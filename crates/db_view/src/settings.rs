use gpui::App;

const DEFAULT_DB_UNDO_STACK_SIZE: usize = 50;

#[derive(Clone, Copy, Debug)]
pub struct DbViewSettings {
    pub db_undo_stack_size: usize,
}

impl Default for DbViewSettings {
    fn default() -> Self {
        Self {
            db_undo_stack_size: DEFAULT_DB_UNDO_STACK_SIZE,
        }
    }
}

impl gpui::Global for DbViewSettings {}

pub fn set_db_view_settings(cx: &mut App, db_undo_stack_size: usize) {
    let next = DbViewSettings { db_undo_stack_size };
    if cx.has_global::<DbViewSettings>() {
        *cx.global_mut::<DbViewSettings>() = next;
    } else {
        cx.set_global(next);
    }
}

pub fn current_db_undo_stack_size(cx: &App) -> usize {
    cx.try_global::<DbViewSettings>()
        .map(|settings| settings.db_undo_stack_size)
        .unwrap_or(DEFAULT_DB_UNDO_STACK_SIZE)
}
