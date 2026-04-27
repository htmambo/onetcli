use std::ops::Range;

use crate::{
    ActiveTheme, Sizable, Size,
    actions::{
        Cancel, SelectDown, SelectFirst, SelectLast, SelectNextColumn, SelectPageDown,
        SelectPageUp, SelectPrevColumn, SelectUp,
    },
};
use gpui::{
    App, Edges, Entity, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement,
    RenderOnce, Styled, Window, div, prelude::FluentBuilder,
};

mod column;
mod delegate;
mod loading;
mod state;

pub use column::*;
pub use delegate::*;
pub use state::*;

const CONTEXT: &'static str = "Table";

/// A trait that defines the data model interface for tables.
///
/// This trait extracts the common data-related functionality from both
/// `TableDelegate` and `EditTableDelegate`, allowing for shared behavior
/// between the basic `Table` and the editable `EditTable` components.
///
/// The trait uses an associated type `Column` to allow each implementation
/// to use its own column type without requiring a unified Column struct.
#[allow(unused)]
pub trait TableModel: Send {
    /// The column type used by this table model.
    type Column;

    /// Return the number of columns in the table.
    fn columns_count(&self) -> usize;

    /// Return the number of rows in the table.
    fn rows_count(&self) -> usize;

    /// Returns the table column at the given index.
    fn column(&self, index: usize) -> Self::Column;

    /// Perform sort on the column at the given index.
    fn perform_sort(&mut self, column: usize, ascending: bool);

    /// Move the column at the given index to a new position.
    fn move_column(&mut self, from: usize, to: usize);

    /// Return true if the table is currently loading data.
    fn loading(&self) -> bool;

    /// Return a view to display while loading, if any.
    fn render_loading(&self) -> Option<gpui::AnyView> {
        None
    }

    /// Return true if there is more data to load (for infinite scroll).
    fn has_more(&self) -> bool {
        false
    }

    /// Returns the threshold (in rows) that triggers loading more data.
    ///
    /// When the visible range is within this many rows from the end,
    /// `load_more` will be called.
    fn load_more_threshold(&self) -> Option<usize> {
        Some(20)
    }

    /// Load more data when triggered by scroll position.
    fn load_more(&mut self) {}

    /// Called when the visible range of rows changes.
    fn visible_rows_changed(&mut self, range: Range<usize>) {}

    /// Called when the visible range of columns changes.
    fn visible_columns_changed(&mut self, range: Range<usize>) {}
}

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("left", SelectPrevColumn, Some(CONTEXT)),
        KeyBinding::new("right", SelectNextColumn, Some(CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(CONTEXT)),
        KeyBinding::new("pageup", SelectPageUp, Some(CONTEXT)),
        KeyBinding::new("pagedown", SelectPageDown, Some(CONTEXT)),
        KeyBinding::new("tab", SelectNextColumn, Some(CONTEXT)),
        KeyBinding::new("shift-tab", SelectPrevColumn, Some(CONTEXT)),
    ]);
}

struct TableOptions {
    scrollbar_visible: Edges<bool>,
    /// Set stripe style of the table.
    stripe: bool,
    /// Set to use border style of the table.
    bordered: bool,
    /// The cell size of the table.
    size: Size,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            scrollbar_visible: Edges::all(true),
            stripe: false,
            bordered: true,
            size: Size::default(),
        }
    }
}

/// A table element with support for row, column, and cell selection.
///
/// # Features
///
/// - **Multiple Selection Modes**: Support for row, column, and cell selection
/// - **Cell Selection**: Click to select individual cells, with keyboard navigation
/// - **Virtual Scrolling**: Efficient rendering of large datasets
/// - **Resizable Columns**: Drag column borders to resize
/// - **Movable Columns**: Drag column headers to reorder
/// - **Fixed Columns**: Pin columns to the left side
/// - **Sortable Columns**: Click column headers to sort
/// - **Context Menus**: Right-click support for rows and cells
///
/// # Cell Selection Mode
///
/// When cell selection is enabled via [`TableState::cell_selectable()`]:
/// - Click on cells to select them
/// - A row selector column appears on the left for selecting entire rows
/// - Keyboard navigation (arrow keys, Tab, Home, End, PageUp, PageDown) works at cell level
/// - Right-click and double-click events are supported
///
/// See [`TableState`] for more details on cell selection.
///
/// # Example
///
/// ```rust,ignore
/// let table_state = cx.new(|cx| {
///     TableState::new(delegate, cx)
///         .cell_selectable(true)
///         .row_selectable(true)
/// });
///
/// Table::new(&table_state)
///     .stripe(true)
///     .bordered(true)
/// ```
#[derive(IntoElement)]
pub struct Table<D: TableDelegate> {
    state: Entity<TableState<D>>,
    options: TableOptions,
}

impl<D> Table<D>
where
    D: TableDelegate,
{
    /// Create a new Table element with the given [`TableState`].
    pub fn new(state: &Entity<TableState<D>>) -> Self {
        Self {
            state: state.clone(),
            options: TableOptions::default(),
        }
    }

    /// Set to use stripe style of the table, default to false.
    pub fn stripe(mut self, stripe: bool) -> Self {
        self.options.stripe = stripe;
        self
    }

    /// Set to use border style of the table, default to true.
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.options.bordered = bordered;
        self
    }

    /// Set scrollbar visibility.
    pub fn scrollbar_visible(mut self, vertical: bool, horizontal: bool) -> Self {
        self.options.scrollbar_visible = Edges {
            right: vertical,
            bottom: horizontal,
            ..Default::default()
        };
        self
    }
}

impl<D> Sizable for Table<D>
where
    D: TableDelegate,
{
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.options.size = size.into();
        self
    }
}

impl<D> RenderOnce for Table<D>
where
    D: TableDelegate,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let bordered = self.options.bordered;
        let focus_handle = self.state.focus_handle(cx);
        self.state.update(cx, |state, _| {
            state.options = self.options;
        });

        div()
            .id("table")
            .size_full()
            .key_context(CONTEXT)
            .track_focus(&focus_handle)
            .on_action(window.listener_for(&self.state, TableState::action_cancel))
            .on_action(window.listener_for(&self.state, TableState::action_select_next))
            .on_action(window.listener_for(&self.state, TableState::action_select_prev))
            .on_action(window.listener_for(&self.state, TableState::action_select_next_col))
            .on_action(window.listener_for(&self.state, TableState::action_select_prev_col))
            .on_action(window.listener_for(&self.state, TableState::action_select_first_column))
            .on_action(window.listener_for(&self.state, TableState::action_select_last_column))
            .on_action(window.listener_for(&self.state, TableState::action_select_page_up))
            .on_action(window.listener_for(&self.state, TableState::action_select_page_down))
            .bg(cx.theme().table)
            .when(bordered, |this| {
                this.rounded(cx.theme().radius)
                    .overflow_hidden()
                    .border_1()
                    .border_color(cx.theme().border)
            })
            .child(self.state)
    }
}
