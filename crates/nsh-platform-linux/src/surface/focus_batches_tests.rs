//! Batch focus and selectable ownership probes. These keep the expected
//! contracts even when upstream fails them; no product fixes belong here.

use std::{cell::Cell, rc::Rc, sync::Arc};

use gtk4::{self as gtk, prelude::*};
use nsh_core::{
    AttributedText, Command, MountBatch, MountItem, NodeId, ROOT_ID, TextEngine,
    TextSpan, ViewKind, ViewProps, ViewStyle,
};

use super::Surface;
use crate::gtk_test::{frame, run, show, surface, view, wait_until};

fn props(id: NodeId, focusable: bool, selectable: bool, disabled: bool) -> MountItem {
    let mut props = ViewProps {
        focusable: Some(focusable),
        selectable_view: selectable,
        ..Default::default()
    };
    props.accessibility.state.disabled = disabled;
    MountItem::ViewProps { id, props: Box::new(props) }
}

fn hidden(id: NodeId, hidden: bool) -> MountItem {
    MountItem::ViewStyle {
        id,
        style: Box::new(ViewStyle { hidden, ..Default::default() }),
    }
}

fn command(id: NodeId, command: Command) -> MountItem {
    MountItem::Command { id, command }
}

// gtk_test::apply deliberately skips the batch end. These contracts include
// end-of-batch selection maintenance, so exercise Surface::apply itself.
fn batch(surface: &Rc<Surface>, items: Vec<MountItem>) {
    surface.apply(&MountBatch { items });
}

struct Fixture {
    surface: Rc<Surface>,
    window: gtk::Window,
    visits: Rc<Cell<u32>>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.window.destroy();
    }
}

impl Fixture {
    fn new() -> Self {
        let surface = surface();
        let mut items = Vec::new();
        for (id, kind) in [(2, ViewKind::View), (3, ViewKind::View),
            (4, ViewKind::Input), (5, ViewKind::View), (7, ViewKind::View)] {
            items.extend([
                MountItem::Create { id, kind },
                MountItem::Insert { parent: ROOT_ID, child: id, before: None },
                MountItem::Frame { id, frame: frame(0.0, (id - 2) as f32 * 30.0, 180.0, 25.0) },
                props(id, id != 3, false, false),
            ]);
        }
        items.push(MountItem::Input { id: 4, config: Box::default() });
        batch(&surface, items);
        let visits = Rc::new(Cell::new(0));
        let counted = visits.clone();
        let observer = gtk::EventControllerFocus::new();
        observer.connect_is_focus_notify(move |c| {
            if c.is_focus() { counted.set(counted.get() + 1); }
        });
        view(&surface, 5).add_controller(observer);
        let window = show(&surface);
        assert!(wait_until(|| [2, 3, 4, 5, 7].iter().all(|id| view(&surface, *id).is_mapped())),
            "bounded readiness: every test view must map");
        Self { surface, window, visits }
    }

    fn apply(&self, items: Vec<MountItem>) { batch(&self.surface, items); }

    fn focused(&self, id: NodeId) -> bool {
        let view = view(&self.surface, id);
        GtkWindowExt::focus(&self.window).is_some_and(|w| w == *view.upcast_ref::<gtk::Widget>() || w.is_ancestor(&view))
    }

    fn reset(&self) {
        self.apply(vec![props(2, true, false, false), props(3, false, false, false),
            props(7, true, false, false), hidden(7, false), command(2, Command::Focus)]);
        assert!(self.focused(2));
        self.visits.set(0);
    }

    fn selection(&self) {
        let text = Arc::new(AttributedText {
            text: "selection survives".into(),
            spans: vec![TextSpan { start: 0, end: 18, style: Default::default() }],
            ..Default::default()
        });
        let engine = crate::text::PangoTextEngine::new("sans-serif".into());
        let bounds = frame(0.0, 0.0, 180.0, 25.0);
        let layout = engine.layout(&engine.prepare(&text), bounds.size);
        self.apply(vec![
            MountItem::Create { id: 20, kind: ViewKind::View },
            MountItem::Insert { parent: ROOT_ID, child: 20, before: None },
            MountItem::Frame { id: 20, frame: bounds },
            props(20, false, true, false),
            MountItem::Create { id: 21, kind: ViewKind::Text },
            MountItem::Insert { parent: 20, child: 21, before: None },
            MountItem::Frame { id: 21, frame: bounds },
            MountItem::Text { id: 21, text, layout, content: bounds },
        ]);
        assert!(wait_until(|| view(&self.surface, 21).is_mapped()));
    }

    fn select(&self) {
        view(&self.surface, 20).activate_action("selection.select-all", None).unwrap();
        self.selected();
    }

    fn selected(&self) {
        let area = view(&self.surface, 20);
        assert!(self.focused(20));
        assert!(crate::selectable_view::has_selection(&self.surface, &area));
        let text = view(&self.surface, 21);
        assert_eq!(text.state().area_selection, Some((0, 18)));
        assert_eq!(text.state().text.as_ref().unwrap().text.text.to_string(), "selection survives");
        assert!(!area.state().props.focus_stop(false));
    }
}

#[test]
fn real_editor_is_mapped_in_its_window() {
    run(|| {
        let f = Fixture::new();
        let editor = view(&f.surface, 4);
        let native = editor.state().natives.iter().find(|n| n.widget.is::<gtk::Text>())
            .expect("a real editor").widget.clone();
        assert_eq!(native.parent().as_ref(), Some(editor.upcast_ref()));
        assert!(native.is_mapped());
        assert_eq!(native.root().as_ref(), Some(f.window.upcast_ref::<gtk::Root>()));
        f.reset();
    });
}

fn handoff(early: bool) {
    run(move || {
        let f = Fixture::new();
        f.reset();
        let mut items = vec![props(2, false, false, false), props(3, true, false, false)];
        items.insert(if early { 0 } else { items.len() }, command(3, Command::Focus));
        f.apply(items);
        assert!(f.focused(3), "early={early}");
        assert_eq!(f.visits.get(), 0, "early={early}");
    });
}

#[test]
fn early_focus_uses_final_props_without_visiting_a_sentinel() { handoff(true); }

#[test]
fn late_focus_uses_final_props_without_visiting_a_sentinel() { handoff(false); }

#[test]
fn following_batch_without_focus_restores_ordinary_fallback() {
    run(|| {
        let f = Fixture::new();
        f.reset();
        f.apply(vec![command(3, Command::Focus), props(3, true, false, false)]);
        f.reset();
        f.apply(vec![props(3, true, false, false)]);
        f.apply(vec![props(2, false, false, false)]);
        assert!(wait_until(|| f.focused(3)));
    });
}

#[test]
fn plain_input_and_blur_commands_keep_their_order() {
    run(|| {
        let f = Fixture::new();
        f.apply(vec![props(3, true, false, false), command(3, Command::Focus), command(4, Command::Focus)]);
        assert!(f.focused(4));
        f.apply(vec![command(4, Command::Focus), command(3, Command::Focus)]);
        assert!(f.focused(3));
        f.apply(vec![command(3, Command::Focus), command(4, Command::Focus), command(4, Command::Blur)]);
        assert!(GtkWindowExt::focus(&f.window).is_none());
        f.apply(vec![command(4, Command::Focus), command(4, Command::Blur), command(3, Command::Focus)]);
        assert!(f.focused(3));
    });
}

#[test]
fn eligible_assistive_focus_keeps_command_order() {
    run(|| {
        let f = Fixture::new();
        f.apply(vec![props(3, true, false, false), command(3, Command::Focus), command(5, Command::AccessibilityFocus)]);
        assert!(f.focused(5));
        f.apply(vec![command(5, Command::AccessibilityFocus), command(3, Command::Focus)]);
        assert!(f.focused(3));
    });
}

fn final_request(refusal: u8) {
    run(move || {
        let f = Fixture::new();
        f.reset();
        let mut items = vec![props(3, true, false, false), command(3, Command::Focus), command(7, Command::Focus)];
        match refusal {
            0 => items.push(props(7, false, false, false)),
            1 => items.push(props(7, true, false, true)),
            2 => items.push(hidden(7, true)),
            _ => {}
        }
        f.apply(items);
        assert!(f.focused(if refusal < 3 { 3 } else { 7 }));
    });
}

#[test]
fn a_nonfocusable_final_request_does_not_erase_a_valid_one() { final_request(0); }

#[test]
fn a_disabled_final_request_does_not_erase_a_valid_one() { final_request(1); }

#[test]
fn a_hidden_final_request_does_not_erase_a_valid_one() { final_request(2); }

#[test]
fn an_accepted_final_request_wins() { final_request(3); }

#[test]
fn a_refused_handoff_clears_invalid_focus_without_sentinel_visits() {
    run(|| {
        let f = Fixture::new();
        f.reset();
        f.apply(vec![command(7, Command::Focus), props(2, false, false, false), hidden(7, true)]);
        assert!(GtkWindowExt::focus(&f.window).is_none());
        assert_eq!(f.visits.get(), 0);
    });
}

fn preserve_selection(assistive: bool) {
    run(move || {
        let f = Fixture::new();
        f.selection();
        f.select();
        let mut items = vec![command(99, Command::Focus)];
        if assistive { items.push(command(20, Command::AccessibilityFocus)); }
        f.apply(items);
        f.selected();
    });
}

#[test]
fn invalid_focus_preserves_selected_text() { preserve_selection(false); }

#[test]
fn assistive_focus_preserves_selected_text() { preserve_selection(true); }

#[test]
fn selection_uninstall_keeps_an_explicit_stop_then_obeys_exclusion() {
    run(|| {
        let f = Fixture::new();
        f.selection();
        f.select();
        f.apply(vec![props(20, true, false, false)]);
        assert!(!view(&f.surface, 20).state().props.selectable_view);
        assert!(view(&f.surface, 21).state().area_selection.is_none());
        assert!(crate::key_focus::is_stop(&view(&f.surface, 20)));
        f.apply(vec![command(20, Command::Focus)]);
        assert!(f.focused(20));
        f.apply(vec![props(20, false, false, false)]);
        assert!(!view(&f.surface, 20).is_focusable());
    });
}

fn selection_refusal(hide: bool) {
    run(move || {
        let f = Fixture::new();
        f.selection();
        assert!(!view(&f.surface, 20).is_focusable());
        f.select();
        f.apply(vec![command(99, Command::Focus), if hide { hidden(20, true) } else { props(20, false, true, true) }]);
        assert!(!view(&f.surface, 20).is_focusable());
        assert!(!f.focused(20));
        assert!(view(&f.surface, 21).state().area_selection.is_none());
        f.apply(vec![hidden(20, false), props(20, false, true, false)]);
        assert!(!view(&f.surface, 20).is_focusable());
        assert!(view(&f.surface, 21).state().area_selection.is_none());
    });
}

#[test]
fn hidden_selection_cannot_resurrect_on_repair() { selection_refusal(true); }

#[test]
fn disabled_selection_cannot_resurrect_on_repair() { selection_refusal(false); }

#[test]
fn empty_dual_owner_gains_and_loses_ordinary_focus() {
    run(|| {
        let f = Fixture::new();
        f.selection();
        let area = view(&f.surface, 20);
        let text = view(&f.surface, 21);
        assert!(area.state().props.selectable_view);
        assert!(!area.is_focusable());
        assert!(text.state().area_selection.is_none());
        f.apply(vec![props(20, true, true, false)]);
        assert!(area.state().props.selectable_view);
        assert!(crate::key_focus::is_stop(&area));
        f.apply(vec![command(20, Command::Focus)]);
        assert!(f.focused(20));
        assert!(text.state().area_selection.is_none());
        f.apply(vec![props(20, false, true, false)]);
        assert!(area.state().props.selectable_view);
        assert!(!area.state().props.focus_stop(false));
        assert!(text.state().area_selection.is_none());
        assert!(!area.is_focusable());
        assert!(!f.focused(20));
    });
}

#[test]
fn another_surfaces_invalid_focus_does_not_clear_selection() {
    run(|| {
        let f = Fixture::new();
        f.selection();
        f.select();
        let other = surface();
        batch(&other, vec![
            MountItem::Create { id: 2, kind: ViewKind::View },
            MountItem::Insert { parent: ROOT_ID, child: 2, before: None },
            MountItem::Frame { id: 2, frame: frame(0.0, 0.0, 180.0, 25.0) },
        ]);
        f.window.set_child(None::<&gtk::Widget>);
        let host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        host.append(&f.surface.root);
        host.append(&other.root);
        f.window.set_child(Some(&host));
        assert!(wait_until(|| view(&other, 2).is_mapped() && view(&f.surface, 21).is_mapped()));
        f.select();
        batch(&other, vec![command(99, Command::Focus)]);
        f.selected();
    });
}
