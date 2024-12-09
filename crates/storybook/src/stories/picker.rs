use fuzzy::StringMatchCandidate;
use gpui::{div, prelude::*, KeyBinding, Render, SharedString, Styled, Task, View};
use picker::{Picker, PickerDelegate};
use std::sync::Arc;
use ui::{prelude::*, ListItemSpacing};
use ui::{Label, ListItem};

pub struct PickerStory {
    picker: Model<Picker<Delegate>>,
}

struct Delegate {
    candidates: Arc<[StringMatchCandidate]>,
    matches: Vec<usize>,
    selected_ix: usize,
}

impl Delegate {
    fn new(strings: &[&str]) -> Self {
        Self {
            candidates: strings
                .iter()
                .copied()
                .enumerate()
                .map(|(id, string)| StringMatchCandidate {
                    id,
                    char_bag: string.into(),
                    string: string.into(),
                })
                .collect(),
            matches: vec![],
            selected_ix: 0,
        }
    }
}

impl PickerDelegate for Delegate {
    type ListItem = ListItem;

    fn match_count(&self) -> usize {
        self.candidates.len()
    }

    fn placeholder_text(&self, _window: &mut gpui::Window, _cx: &mut gpui::AppContext) -> Arc<str> {
        "Test".into()
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        model: &Model<Picker>,
        _cx: &mut AppContext,
    ) -> Option<Self::ListItem> {
        let candidate_ix = self.matches.get(ix)?;
        // TASK: Make StringMatchCandidate::string a SharedString
        let candidate = SharedString::from(self.candidates[*candidate_ix].string.clone());

        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .selected(selected)
                .child(Label::new(candidate)),
        )
    }

    fn selected_index(&self) -> usize {
        self.selected_ix
    }

    fn set_selected_index(&mut self, ix: usize, model: &Model<Picker>, cx: &mut AppContext) {
        self.selected_ix = ix;
        model.notify(cx);
    }

    fn confirm(&mut self, secondary: bool, model: &Model<Picker>, _cx: &mut AppContext) {
        let candidate_ix = self.matches[self.selected_ix];
        let candidate = self.candidates[candidate_ix].string.clone();

        if secondary {
            eprintln!("Secondary confirmed {}", candidate)
        } else {
            eprintln!("Confirmed {}", candidate)
        }
    }

    fn dismissed(&mut self, model: &Model<Picker>, cx: &mut AppContext) {
        cx.quit();
    }

    fn update_matches(
        &mut self,
        query: String,
        model: &Model<Picker>,
        cx: &mut AppContext,
    ) -> Task<()> {
        let candidates = self.candidates.clone();
        self.matches = cx
            .background_executor()
            .block(fuzzy::match_strings(
                &candidates,
                &query,
                true,
                100,
                &Default::default(),
                cx.background_executor().clone(),
            ))
            .into_iter()
            .map(|r| r.candidate_id)
            .collect();
        self.selected_ix = 0;
        Task::ready(())
    }
}

impl PickerStory {
    pub fn new(window: &mut gpui::Window, cx: &mut gpui::AppContext) -> Model<Self> {
        cx.new_model(|model, cx| {
            cx.bind_keys([
                KeyBinding::new("up", menu::SelectPrev, Some("picker")),
                KeyBinding::new("pageup", menu::SelectFirst, Some("picker")),
                KeyBinding::new("shift-pageup", menu::SelectFirst, Some("picker")),
                KeyBinding::new("ctrl-p", menu::SelectPrev, Some("picker")),
                KeyBinding::new("down", menu::SelectNext, Some("picker")),
                KeyBinding::new("pagedown", menu::SelectLast, Some("picker")),
                KeyBinding::new("shift-pagedown", menu::SelectFirst, Some("picker")),
                KeyBinding::new("ctrl-n", menu::SelectNext, Some("picker")),
                KeyBinding::new("cmd-up", menu::SelectFirst, Some("picker")),
                KeyBinding::new("cmd-down", menu::SelectLast, Some("picker")),
                KeyBinding::new("enter", menu::Confirm, Some("picker")),
                KeyBinding::new("ctrl-enter", menu::SecondaryConfirm, Some("picker")),
                KeyBinding::new("cmd-enter", menu::SecondaryConfirm, Some("picker")),
                KeyBinding::new("escape", menu::Cancel, Some("picker")),
                KeyBinding::new("ctrl-c", menu::Cancel, Some("picker")),
            ]);

            PickerStory {
                picker: cx.new_model(|model, cx| {
                    let mut delegate = Delegate::new(&[
                        "Baguette (France)",
                        "Baklava (Turkey)",
                        "Beef Wellington (UK)",
                        "Biryani (India)",
                        "Borscht (Ukraine)",
                        "Bratwurst (Germany)",
                        "Bulgogi (Korea)",
                        "Burrito (USA)",
                        "Ceviche (Peru)",
                        "Chicken Tikka Masala (India)",
                        "Churrasco (Brazil)",
                        "Couscous (North Africa)",
                        "Croissant (France)",
                        "Dim Sum (China)",
                        "Empanada (Argentina)",
                        "Fajitas (Mexico)",
                        "Falafel (Middle East)",
                        "Feijoada (Brazil)",
                        "Fish and Chips (UK)",
                        "Fondue (Switzerland)",
                        "Goulash (Hungary)",
                        "Haggis (Scotland)",
                        "Kebab (Middle East)",
                        "Kimchi (Korea)",
                        "Lasagna (Italy)",
                        "Maple Syrup Pancakes (Canada)",
                        "Moussaka (Greece)",
                        "Pad Thai (Thailand)",
                        "Paella (Spain)",
                        "Pancakes (USA)",
                        "Pasta Carbonara (Italy)",
                        "Pavlova (Australia)",
                        "Peking Duck (China)",
                        "Pho (Vietnam)",
                        "Pierogi (Poland)",
                        "Pizza (Italy)",
                        "Poutine (Canada)",
                        "Pretzel (Germany)",
                        "Ramen (Japan)",
                        "Rendang (Indonesia)",
                        "Sashimi (Japan)",
                        "Satay (Indonesia)",
                        "Shepherd's Pie (Ireland)",
                        "Sushi (Japan)",
                        "Tacos (Mexico)",
                        "Tandoori Chicken (India)",
                        "Tortilla (Spain)",
                        "Tzatziki (Greece)",
                        "Wiener Schnitzel (Austria)",
                    ]);
                    delegate.update_matches("".into(), model, cx).detach();

                    let picker = Picker::uniform_list(delegate, model, cx);
                    picker.focus(model, cx);
                    picker
                }),
            }
        })
    }
}

impl Render for PickerStory {
    fn render(&mut self, model: &Model<Self>, window: &mut gpui::Window, cx: &mut AppContext) -> impl IntoElement {
        div()
            .bg(cx.theme().styles.colors.background)
            .size_full()
            .child(self.picker.clone())
    }
}
