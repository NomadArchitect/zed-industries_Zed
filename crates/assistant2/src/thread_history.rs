use gpui::{
    uniform_list, AppContext, FocusHandle, FocusableView, Model, UniformListScrollHandle, WeakView,
};
use time::{OffsetDateTime, UtcOffset};
use ui::{prelude::*, IconButtonShape, ListItem};

use crate::thread::Thread;
use crate::thread_store::ThreadStore;
use crate::AssistantPanel;

pub struct ThreadHistory {
    focus_handle: FocusHandle,
    assistant_panel: WeakModel<AssistantPanel>,
    thread_store: Model<ThreadStore>,
    scroll_handle: UniformListScrollHandle,
}

impl ThreadHistory {
    pub(crate) fn new(
        assistant_panel: WeakModel<AssistantPanel>,
        thread_store: Model<ThreadStore>,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) -> Self {
        Self {
            focus_handle: window.focus_handle(),
            assistant_panel,
            thread_store,
            scroll_handle: UniformListScrollHandle::default(),
        }
    }
}

impl FocusableView for ThreadHistory {
    fn focus_handle(&self, _cx: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ThreadHistory {
    fn render(
        &mut self,
        model: &Model<Self>,
        window: &mut gpui::Window,
        cx: &mut AppContext,
    ) -> impl IntoElement {
        let threads = self
            .thread_store
            .update(cx, |this, model, cx| this.threads(cx));

        v_flex()
            .id("thread-history-container")
            .track_focus(&self.focus_handle)
            .overflow_y_scroll()
            .size_full()
            .p_1()
            .map(|history| {
                if threads.is_empty() {
                    history
                        .justify_center()
                        .child(
                            h_flex().w_full().justify_center().child(
                                Label::new("You don't have any past threads yet.")
                                    .size(LabelSize::Small),
                            ),
                        )
                } else {
                    history.child(
                        uniform_list(
                            cx.view().clone(),
                            "thread-history",
                            threads.len(),
                            move |history, range, _cx| {
                                threads[range]
                                    .iter()
                                    .map(|thread| {
                                        PastThread::new(
                                            thread.clone(),
                                            history.assistant_panel.clone(),
                                        )
                                    })
                                    .collect()
                            },
                        )
                        .track_scroll(self.scroll_handle.clone())
                        .flex_grow(),
                    )
                }
            })
    }
}

#[derive(IntoElement)]
pub struct PastThread {
    thread: Model<Thread>,
    assistant_panel: WeakModel<AssistantPanel>,
}

impl PastThread {
    pub fn new(thread: Model<Thread>, assistant_panel: WeakModel<AssistantPanel>) -> Self {
        Self {
            thread,
            assistant_panel,
        }
    }
}

impl RenderOnce for PastThread {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::AppContext) -> impl IntoElement {
        let (id, summary) = {
            const DEFAULT_SUMMARY: SharedString = SharedString::new_static("New Thread");
            let thread = self.thread.read(cx);
            (
                thread.id().clone(),
                thread.summary().unwrap_or(DEFAULT_SUMMARY),
            )
        };

        let thread_timestamp = time_format::format_localized_timestamp(
            OffsetDateTime::from_unix_timestamp(self.thread.read(cx).updated_at().timestamp())
                .unwrap(),
            OffsetDateTime::now_utc(),
            self.assistant_panel
                .update(cx, |this, model, _cx| this.local_timezone())
                .unwrap_or(UtcOffset::UTC),
            time_format::TimestampFormat::EnhancedAbsolute,
        );
        ListItem::new(("past-thread", self.thread.entity_id()))
            .start_slot(Icon::new(IconName::MessageBubbles))
            .child(Label::new(summary))
            .end_slot(
                h_flex()
                    .gap_2()
                    .child(Label::new(thread_timestamp).color(Color::Disabled))
                    .child(
                        IconButton::new("delete", IconName::TrashAlt)
                            .shape(IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .on_click({
                                let assistant_panel = self.assistant_panel.clone();
                                let id = id.clone();
                                move |_event, cx| {
                                    assistant_panel
                                        .update(cx, |this, model, cx| {
                                            this.delete_thread(&id, cx);
                                        })
                                        .ok();
                                }
                            }),
                    ),
            )
            .on_click({
                let assistant_panel = self.assistant_panel.clone();
                let id = id.clone();
                move |_event, cx| {
                    assistant_panel
                        .update(cx, |this, model, cx| {
                            this.open_thread(&id, cx);
                        })
                        .ok();
                }
            })
    }
}
