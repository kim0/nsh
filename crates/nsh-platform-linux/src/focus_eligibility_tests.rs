//! Public mount stimuli for a focused stop losing its eligibility.
//! Native focus and the NSH focus state are separate observations; no test
//! requires GTK to pick the adjacent stop automatically.

use gtk4::{self as gtk, prelude::*};
use nsh_core::{
    Command, EventKind, EventMask, MountBatch, MountItem, ROOT_ID, ViewKind, ViewProps, ViewStyle,
    tailwind::state,
};

use crate::gtk_test::{frame, run, show, surface, view, wait_until};

fn loses_focus(change: MountItem) {
    let surface = surface();
    let mut items = Vec::new();
    for (id, y) in [(2, 0.0), (3, 40.0)] {
        let mut events = EventMask::default();
        events.set(EventKind::Press, true);
        items.extend([
            MountItem::Create {
                id,
                kind: ViewKind::View,
            },
            MountItem::Insert {
                parent: ROOT_ID,
                child: id,
                before: None,
            },
            MountItem::Frame {
                id,
                frame: frame(0.0, y, 100.0, 30.0),
            },
            MountItem::Events { id, events },
        ]);
    }
    surface.apply(&MountBatch { items });
    let window = show(&surface);
    let first = view(&surface, 2);
    let neighbor = view(&surface, 3);
    assert!(wait_until(|| first.is_mapped() && neighbor.is_mapped()));
    let command = |id| MountBatch {
        items: vec![MountItem::Command {
            id,
            command: Command::Focus,
        }],
    };
    // Prove the neighbor accepts explicit focus before testing invalidation.
    surface.apply(&command(3));
    assert!(wait_until(|| neighbor.is_focus()));
    surface.apply(&command(2));
    assert!(wait_until(|| first.is_focus()));
    assert_ne!(first.state().states & state::FOCUSED, 0);

    surface.apply(&MountBatch {
        items: vec![change],
    });
    let cleared = wait_until(|| !first.is_focus() && first.state().states & state::FOCUSED == 0);
    // Capture both observations before any failure, and close even on red.
    let native_focus = GtkWindowExt::focus(&window).map(|widget| {
        (
            widget.type_().name().to_string(),
            widget == *first.upcast_ref::<gtk::Widget>(),
            widget == *neighbor.upcast_ref::<gtk::Widget>(),
        )
    });
    let diagnostic = format!(
        "native={native_focus:?}, source_focusable={}, logical_stop={}, \
         gtk_visible={}, style_hidden={}, disabled={}, focus_state={}",
        first.is_focusable(),
        crate::key_focus::is_stop(&first),
        first.is_visible(),
        first.state().style.hidden,
        first.state().props.accessibility.state.disabled,
        first.state().states & state::FOCUSED != 0,
    );
    window.destroy();
    assert!(
        cleared,
        "ineligible stop kept native focus or NSH focus state: {diagnostic}"
    );
}

#[test]
fn focusable_false_releases_native_focus_with_press_listener_retained() {
    run(|| {
        loses_focus(MountItem::ViewProps {
            id: 2,
            props: Box::new(ViewProps {
                focusable: Some(false),
                ..Default::default()
            }),
        });
    });
}

#[test]
fn removing_the_only_press_listener_releases_native_focus() {
    run(|| {
        loses_focus(MountItem::Events {
            id: 2,
            events: EventMask::default(),
        });
    });
}

#[test]
fn disabling_a_focused_stop_releases_native_focus() {
    run(|| {
        let mut props = ViewProps::default();
        props.accessibility.state.disabled = true;
        loses_focus(MountItem::ViewProps {
            id: 2,
            props: Box::new(props),
        });
    });
}

#[test]
fn hiding_a_focused_stop_releases_native_focus() {
    run(|| {
        // This tests the mounted hidden style, NOT GtkWidget::set_visible.
        // Report both visibility channels: their equivalence is not assumed.
        loses_focus(MountItem::ViewStyle {
            id: 2,
            style: Box::new(ViewStyle {
                hidden: true,
                ..Default::default()
            }),
        });
    });
}
