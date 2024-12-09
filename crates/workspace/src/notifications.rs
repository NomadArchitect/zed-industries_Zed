use crate::{Toast, Workspace};
use collections::HashMap;
use gpui::{
    svg, AnyView, AppContext, ClipboardItem, DismissEvent, Entity, EntityId, EventEmitter, Global,
    Model, PromptLevel, Render, ScrollHandle, Task, View, VisualContext,
};
use language::DiagnosticSeverity;

use std::{any::TypeId, ops::DerefMut, time::Duration};
use ui::{prelude::*, Tooltip};
use util::ResultExt;

pub fn init(cx: &mut AppContext) {
    cx.set_global(NotificationTracker::new());
}

#[derive(Debug, PartialEq, Clone)]
pub enum NotificationId {
    Unique(TypeId),
    Composite(TypeId, ElementId),
    Named(SharedString),
}

impl NotificationId {
    /// Returns a unique [`NotificationId`] for the given type.
    pub fn unique<T: 'static>() -> Self {
        Self::Unique(TypeId::of::<T>())
    }

    /// Returns a [`NotificationId`] for the given type that is also identified
    /// by the provided ID.
    pub fn composite<T: 'static>(id: impl Into<ElementId>) -> Self {
        Self::Composite(TypeId::of::<T>(), id.into())
    }

    /// Builds a `NotificationId` out of the given string.
    pub fn named(id: SharedString) -> Self {
        Self::Named(id)
    }
}

pub trait Notification: EventEmitter<DismissEvent> + Render {}

impl<V: EventEmitter<DismissEvent> + Render> Notification for V {}

pub trait NotificationHandle: Send {
    fn id(&self) -> EntityId;
    fn to_any(&self) -> AnyView;
}

impl<T: Notification> NotificationHandle for Model<T> {
    fn id(&self) -> EntityId {
        self.entity_id()
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }
}

impl From<&dyn NotificationHandle> for AnyView {
    fn from(val: &dyn NotificationHandle) -> Self {
        val.to_any()
    }
}

pub(crate) struct NotificationTracker {
    notifications_sent: HashMap<TypeId, Vec<NotificationId>>,
}

impl Global for NotificationTracker {}

impl std::ops::Deref for NotificationTracker {
    type Target = HashMap<TypeId, Vec<NotificationId>>;

    fn deref(&self) -> &Self::Target {
        &self.notifications_sent
    }
}

impl DerefMut for NotificationTracker {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.notifications_sent
    }
}

impl NotificationTracker {
    fn new() -> Self {
        Self {
            notifications_sent: Default::default(),
        }
    }
}

impl Workspace {
    pub fn has_shown_notification_once<V: Notification>(
        &self,
        id: &NotificationId,
        model: &Model<Self>,
        cx: &AppContext,
    ) -> bool {
        cx.global::<NotificationTracker>()
            .get(&TypeId::of::<V>())
            .map(|ids| ids.contains(id))
            .unwrap_or(false)
    }

    pub fn show_notification_once<V: Notification>(
        &mut self,
        id: NotificationId,
        model: &Model<Self>,
        cx: &mut AppContext,
        build_notification: impl FnOnce(&Model<Self>, &mut AppContext) -> Model<V>,
    ) {
        if !self.has_shown_notification_once::<V>(&id, cx) {
            let tracker = cx.global_mut::<NotificationTracker>();
            let entry = tracker.entry(TypeId::of::<V>()).or_default();
            entry.push(id.clone());
            self.show_notification::<V>(id, cx, build_notification)
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn notification_ids(&self) -> Vec<NotificationId> {
        self.notifications
            .iter()
            .map(|(id, _)| id)
            .cloned()
            .collect()
    }

    pub fn show_notification<V: Notification>(
        &mut self,
        id: NotificationId,
        model: &Model<Self>,
        cx: &mut AppContext,
        build_notification: impl FnOnce(&Model<Self>, &mut AppContext) -> Model<V>,
    ) {
        self.dismiss_notification_internal(&id, cx);

        let notification = build_notification(model, cx);
        cx.subscribe(&notification, {
            let id = id.clone();
            move |this, _, _: &DismissEvent, cx| {
                this.dismiss_notification_internal(&id, cx);
            }
        })
        .detach();
        self.notifications.push((id, Box::new(notification)));
        model.notify(cx);
    }

    pub fn show_error<E>(&mut self, err: &E, model: &Model<Self>, cx: &mut AppContext)
    where
        E: std::fmt::Debug + std::fmt::Display,
    {
        struct WorkspaceErrorNotification;

        self.show_notification(
            NotificationId::unique::<WorkspaceErrorNotification>(),
            cx,
            |cx| cx.new_model(|_model, _cx| ErrorMessagePrompt::new(format!("Error: {err:#}"))),
        );
    }

    pub fn show_portal_error(&mut self, err: String, model: &Model<Self>, cx: &mut AppContext) {
        struct PortalError;

        self.show_notification(NotificationId::unique::<PortalError>(), cx, |cx| {
            cx.new_model(|_model, _cx| {
                ErrorMessagePrompt::new(err.to_string()).with_link_button(
                    "See docs",
                    "https://zed.dev/docs/linux#i-cant-open-any-files",
                )
            })
        });
    }

    pub fn dismiss_notification(
        &mut self,
        id: &NotificationId,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) {
        self.dismiss_notification_internal(id, cx)
    }

    pub fn show_toast(&mut self, toast: Toast, model: &Model<Self>, cx: &mut AppContext) {
        self.dismiss_notification(&toast.id, cx);
        self.show_notification(toast.id.clone(), cx, |cx| {
            cx.new_model(|_model, _cx| match toast.on_click.as_ref() {
                Some((click_msg, on_click)) => {
                    let on_click = on_click.clone();
                    simple_message_notification::MessageNotification::new(toast.msg.clone())
                        .with_click_message(click_msg.clone())
                        .on_click(move |cx| on_click(cx))
                }
                None => simple_message_notification::MessageNotification::new(toast.msg.clone()),
            })
        });
        if toast.autohide {
            cx.spawn(|workspace, mut cx| async move {
                cx.background_executor()
                    .timer(Duration::from_millis(5000))
                    .await;
                workspace
                    .update(&mut cx, |workspace, cx| {
                        workspace.dismiss_toast(&toast.id, cx)
                    })
                    .ok();
            })
            .detach();
        }
    }

    pub fn dismiss_toast(&mut self, id: &NotificationId, model: &Model<Self>, cx: &mut AppContext) {
        self.dismiss_notification(id, cx);
    }

    pub fn clear_all_notifications(&mut self, model: &Model<Self>, cx: &mut AppContext) {
        self.notifications.clear();
        model.notify(cx);
    }

    fn dismiss_notification_internal(
        &mut self,
        id: &NotificationId,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) {
        self.notifications.retain(|(existing_id, _)| {
            if existing_id == id {
                model.notify(cx);
                false
            } else {
                true
            }
        });
    }
}

pub struct LanguageServerPrompt {
    request: Option<project::LanguageServerPromptRequest>,
    scroll_handle: ScrollHandle,
}

impl LanguageServerPrompt {
    pub fn new(request: project::LanguageServerPromptRequest) -> Self {
        Self {
            request: Some(request),
            scroll_handle: ScrollHandle::new(),
        }
    }

    async fn select_option(this: Model<Self>, ix: usize, mut cx: AsyncAppContext) {
        util::maybe!(async move {
            let potential_future = this.update(&mut cx, |this, _, _| {
                this.request.take().map(|request| request.respond(ix))
            });

            potential_future? // App Closed
                .ok_or_else(|| anyhow::anyhow!("Response already sent"))?
                .await
                .ok_or_else(|| anyhow::anyhow!("Stream already closed"))?;

            this.update(&mut cx, |_, cx| model.emit(cx, DismissEvent))?;

            anyhow::Ok(())
        })
        .await
        .log_err();
    }
}

impl Render for LanguageServerPrompt {
    fn render(
        &mut self,
        model: &Model<Self>,
        window: &mut gpui::Window,
        cx: &mut AppContext,
    ) -> impl IntoElement {
        let Some(request) = &self.request else {
            return div().id("language_server_prompt_notification");
        };

        h_flex()
            .id("language_server_prompt_notification")
            .occlude()
            .elevation_3(cx)
            .items_start()
            .justify_between()
            .p_2()
            .gap_2()
            .w_full()
            .max_h(vh(0.8, cx))
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .group("")
            .child(
                v_flex()
                    .w_full()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(
                                h_flex()
                                    .flex_grow()
                                    .children(
                                        match request.level {
                                            PromptLevel::Info => None,
                                            PromptLevel::Warning => {
                                                Some(DiagnosticSeverity::WARNING)
                                            }
                                            PromptLevel::Critical => {
                                                Some(DiagnosticSeverity::ERROR)
                                            }
                                        }
                                        .map(|severity| {
                                            svg()
                                                .size(cx.text_style().font_size)
                                                .flex_none()
                                                .mr_1()
                                                .mt(px(-2.0))
                                                .map(|icon| {
                                                    if severity == DiagnosticSeverity::ERROR {
                                                        icon.path(IconName::Warning.path())
                                                            .text_color(Color::Error.color(cx))
                                                    } else {
                                                        icon.path(IconName::Warning.path())
                                                            .text_color(Color::Warning.color(cx))
                                                    }
                                                })
                                        }),
                                    )
                                    .child(
                                        Label::new(request.lsp_name.clone())
                                            .size(LabelSize::Default),
                                    ),
                            )
                            .child(ui::IconButton::new("close", ui::IconName::Close).on_click(
                                cx.listener(|_, _, cx| model.emit(cx, gpui::DismissEvent)),
                            )),
                    )
                    .child(
                        v_flex()
                            .child(
                                h_flex().absolute().right_0().rounded_md().child(
                                    ui::IconButton::new("copy", ui::IconName::Copy)
                                        .on_click({
                                            let message = request.message.clone();
                                            move |_, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    message.clone(),
                                                ))
                                            }
                                        })
                                        .tooltip(|window, cx| Tooltip::text("Copy", cx))
                                        .visible_on_hover(""),
                                ),
                            )
                            .child(Label::new(request.message.to_string()).size(LabelSize::Small)),
                    )
                    .children(request.actions.iter().enumerate().map(|(ix, action)| {
                        let this_handle = cx.view().clone();
                        ui::Button::new(ix, action.title.clone())
                            .size(ButtonSize::Large)
                            .on_click(move |_, cx| {
                                let this_handle = this_handle.clone();
                                cx.spawn(|cx| async move {
                                    LanguageServerPrompt::select_option(this_handle, ix, cx).await
                                })
                                .detach()
                            })
                    })),
            )
    }
}

impl EventEmitter<DismissEvent> for LanguageServerPrompt {}

pub struct ErrorMessagePrompt {
    message: SharedString,
    label_and_url_button: Option<(SharedString, SharedString)>,
}

impl ErrorMessagePrompt {
    pub fn new<S>(message: S) -> Self
    where
        S: Into<SharedString>,
    {
        Self {
            message: message.into(),
            label_and_url_button: None,
        }
    }

    pub fn with_link_button<S>(mut self, label: S, url: S) -> Self
    where
        S: Into<SharedString>,
    {
        self.label_and_url_button = Some((label.into(), url.into()));
        self
    }
}

impl Render for ErrorMessagePrompt {
    fn render(
        &mut self,
        model: &Model<Self>,
        window: &mut gpui::Window,
        cx: &mut AppContext,
    ) -> impl IntoElement {
        h_flex()
            .id("error_message_prompt_notification")
            .occlude()
            .elevation_3(cx)
            .items_start()
            .justify_between()
            .p_2()
            .gap_2()
            .w_full()
            .child(
                v_flex()
                    .w_full()
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(
                                svg()
                                    .size(cx.text_style().font_size)
                                    .flex_none()
                                    .mr_2()
                                    .mt(px(-2.0))
                                    .map(|icon| {
                                        icon.path(IconName::Warning.path())
                                            .text_color(Color::Error.color(cx))
                                    }),
                            )
                            .child(ui::IconButton::new("close", ui::IconName::Close).on_click(
                                cx.listener(|_, _, cx| model.emit(cx, gpui::DismissEvent)),
                            )),
                    )
                    .child(
                        div()
                            .max_w_80()
                            .child(Label::new(self.message.clone()).size(LabelSize::Small)),
                    )
                    .when_some(self.label_and_url_button.clone(), |elm, (label, url)| {
                        elm.child(
                            div().mt_2().child(
                                ui::Button::new("error_message_prompt_notification_button", label)
                                    .on_click(move |_, cx| cx.open_url(&url)),
                            ),
                        )
                    }),
            )
    }
}

impl EventEmitter<DismissEvent> for ErrorMessagePrompt {}

pub mod simple_message_notification {
    use gpui::{
        div, DismissEvent, EventEmitter, InteractiveElement, Model, ParentElement, Render,
        SharedString, StatefulInteractiveElement, Styled,
    };
    use std::sync::Arc;
    use ui::prelude::*;
    use ui::{h_flex, v_flex, Button, Icon, IconName, Label, StyledExt};

    pub struct MessageNotification {
        message: SharedString,
        on_click: Option<Arc<dyn Fn(&Model<Self>, &mut AppContext)>>,
        click_message: Option<SharedString>,
        secondary_click_message: Option<SharedString>,
        secondary_on_click: Option<Arc<dyn Fn(&Model<Self>, &mut AppContext)>>,
    }

    impl EventEmitter<DismissEvent> for MessageNotification {}

    impl MessageNotification {
        pub fn new<S>(message: S) -> MessageNotification
        where
            S: Into<SharedString>,
        {
            Self {
                message: message.into(),
                on_click: None,
                click_message: None,
                secondary_on_click: None,
                secondary_click_message: None,
            }
        }

        pub fn with_click_message<S>(mut self, message: S) -> Self
        where
            S: Into<SharedString>,
        {
            self.click_message = Some(message.into());
            self
        }

        pub fn on_click<F>(mut self, on_click: F) -> Self
        where
            F: 'static + Fn(&Model<Self>, &mut AppContext),
        {
            self.on_click = Some(Arc::new(on_click));
            self
        }

        pub fn with_secondary_click_message<S>(mut self, message: S) -> Self
        where
            S: Into<SharedString>,
        {
            self.secondary_click_message = Some(message.into());
            self
        }

        pub fn on_secondary_click<F>(mut self, on_click: F) -> Self
        where
            F: 'static + Fn(&Model<Self>, &mut AppContext),
        {
            self.secondary_on_click = Some(Arc::new(on_click));
            self
        }

        pub fn dismiss(&mut self, model: &Model<Self>, cx: &mut AppContext) {
            model.emit(cx, DismissEvent);
        }
    }

    impl Render for MessageNotification {
        fn render(
            &mut self,
            model: &Model<Self>,
            window: &mut gpui::Window,
            cx: &mut AppContext,
        ) -> impl IntoElement {
            v_flex()
                .elevation_3(cx)
                .p_4()
                .child(
                    h_flex()
                        .justify_between()
                        .child(div().max_w_80().child(Label::new(self.message.clone())))
                        .child(
                            div()
                                .id("cancel")
                                .child(Icon::new(IconName::Close))
                                .cursor_pointer()
                                .on_click(model.listener(|this, model, _, cx| this.dismiss(cx))),
                        ),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .children(self.click_message.iter().map(|message| {
                            Button::new(message.clone(), message.clone()).on_click(cx.listener(
                                |this, _, cx| {
                                    if let Some(on_click) = this.on_click.as_ref() {
                                        (on_click)(cx)
                                    };
                                    this.dismiss(cx)
                                },
                            ))
                        }))
                        .children(self.secondary_click_message.iter().map(|message| {
                            Button::new(message.clone(), message.clone())
                                .style(ButtonStyle::Filled)
                                .on_click(model.listener(|this, model, _, cx| {
                                    if let Some(on_click) = this.secondary_on_click.as_ref() {
                                        (on_click)(cx)
                                    };
                                    this.dismiss(cx)
                                }))
                        })),
                )
        }
    }
}

pub trait NotifyResultExt {
    type Ok;

    fn notify_err(
        self,
        workspace: &mut Workspace,
        model: &Model<Workspace>,
        cx: &mut AppContext,
    ) -> Option<Self::Ok>;

    fn notify_async_err(
        self,
        window_handle: AnyWindowHandle,
        cx: &mut AsyncAppContext,
    ) -> Option<Self::Ok>;
}

impl<T, E> NotifyResultExt for Result<T, E>
where
    E: std::fmt::Debug + std::fmt::Display,
{
    type Ok = T;

    fn notify_err(
        self,
        workspace: &mut Workspace,
        model: &Model<Workspace>,
        cx: &mut AppContext,
    ) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!("TODO {err:?}");
                workspace.show_error(&err, cx);
                None
            }
        }
    }

    fn notify_async_err(
        self,
        window_handle: AnyWindowHandle,
        cx: &mut AsyncAppContext,
    ) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!("{err:?}");
                cx.update_root(|view, cx| {
                    if let Ok(workspace) = view.downcast::<Workspace>() {
                        workspace.update(cx, |workspace, model, cx| workspace.show_error(&err, cx))
                    }
                })
                .ok();
                None
            }
        }
    }
}

pub trait NotifyTaskExt {
    fn detach_and_notify_err(self, window: &mut gpui::Window, cx: &mut gpui::AppContext);
}

impl<R, E> NotifyTaskExt for Task<Result<R, E>>
where
    E: std::fmt::Debug + std::fmt::Display + Sized + 'static,
    R: 'static,
{
    fn detach_and_notify_err(self, window: &mut gpui::Window, cx: &mut gpui::AppContext) {
        cx.spawn(|mut cx| async move { self.await.notify_async_err(&mut cx) })
            .detach();
    }
}

pub trait DetachAndPromptErr<R> {
    fn prompt_err(
        self,
        msg: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::AppContext,
        f: impl FnOnce(&anyhow::Error, &mut gpui::Window, &mut gpui::AppContext) -> Option<String>
            + 'static,
    ) -> Task<Option<R>>;

    fn detach_and_prompt_err(
        self,
        msg: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::AppContext,
        f: impl FnOnce(&anyhow::Error, &mut gpui::Window, &mut gpui::AppContext) -> Option<String>
            + 'static,
    );
}

impl<R> DetachAndPromptErr<R> for Task<anyhow::Result<R>>
where
    R: 'static,
{
    fn prompt_err(
        self,
        msg: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::AppContext,
        f: impl FnOnce(&anyhow::Error, &mut gpui::Window, &mut gpui::AppContext) -> Option<String>
            + 'static,
    ) -> Task<Option<R>> {
        let msg = msg.to_owned();
        cx.spawn(|mut cx| async move {
            let result = self.await;
            if let Err(err) = result.as_ref() {
                log::error!("{err:?}");
                if let Ok(prompt) = cx.update(|cx| {
                    let detail = f(err, cx).unwrap_or_else(|| format!("{err}. Please try again."));
                    cx.prompt(PromptLevel::Critical, &msg, Some(&detail), &["Ok"])
                }) {
                    prompt.await.ok();
                }
                return None;
            }
            Some(result.unwrap())
        })
    }

    fn detach_and_prompt_err(
        self,
        msg: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::AppContext,
        f: impl FnOnce(&anyhow::Error, &mut gpui::Window, &mut gpui::AppContext) -> Option<String>
            + 'static,
    ) {
        self.prompt_err(msg, cx, f).detach();
    }
}
