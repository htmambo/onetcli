use std::{ops::Range, rc::Rc};

use gpui::{
    App, Bounds, Corners, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, Half,
    HighlightStyle, Hitbox, Hsla, IntoElement, LayoutId, MouseButton, MouseMoveEvent, Path, Pixels,
    Point, ShapedLine, SharedString, Size, Style, TextAlign, TextRun, TextStyle, UnderlineStyle,
    Window, fill, point, px, relative, size,
};
use ropey::Rope;
use smallvec::SmallVec;

use crate::{
    ActiveTheme as _, Colorize, PixelsExt, Root,
    input::{RopeExt as _, blink_cursor::CURSOR_WIDTH, text_wrapper::LineLayout},
};

use super::{InputState, LastLayout, WhitespaceIndicators, mode::InputMode};

const BOTTOM_MARGIN_ROWS: usize = 3;
pub(super) const RIGHT_MARGIN: Pixels = px(10.);
pub(super) const LINE_NUMBER_RIGHT_MARGIN: Pixels = px(10.);
const MAX_HIGHLIGHT_LINE_LENGTH: usize = 10_000;

pub(super) struct TextElement {
    pub(crate) state: Entity<InputState>,
    placeholder: SharedString,
}

impl TextElement {
    pub(super) fn new(state: Entity<InputState>) -> Self {
        Self {
            state,
            placeholder: SharedString::default(),
        }
    }

    /// Set the placeholder text of the input field.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    fn paint_mouse_listeners(&mut self, window: &mut Window, _: &mut App) {
        window.on_mouse_event({
            let state = self.state.clone();

            move |event: &MouseMoveEvent, _, window, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    state.update(cx, |state, cx| {
                        state.on_drag_move(event, window, cx);
                    });
                }
            }
        });
    }

    /// Returns the:
    ///
    /// - cursor bounds
    /// - scroll offset
    /// - current row index (No only the visible lines, but all lines)
    ///
    /// This method also will update for track scroll to cursor.
    fn layout_cursor(
        &self,
        last_layout: &LastLayout,
        bounds: &mut Bounds<Pixels>,
        _: &mut Window,
        cx: &mut App,
    ) -> (Option<Bounds<Pixels>>, Point<Pixels>, Option<usize>) {
        let state = self.state.read(cx);

        let line_height = last_layout.line_height;
        let visible_range = &last_layout.visible_range;
        let lines = &last_layout.lines;
        let text_wrapper = &state.text_wrapper;
        let line_number_width = last_layout.line_number_width;

        let mut selected_range = state.selected_range;

        if let Some(ime_marked_range) = &state.ime_marked_range {
            selected_range = (ime_marked_range.end..ime_marked_range.end).into();
        }
        let is_selected_all = selected_range.len() == state.text.len();

        let mut cursor = state.cursor();
        if state.masked {
            // Because masked use `*`, 1 char with 1 byte.
            selected_range.start = state.text.offset_to_char_index(selected_range.start);
            selected_range.end = state.text.offset_to_char_index(selected_range.end);
            cursor = state.text.offset_to_char_index(cursor);
        }

        let mut current_row = None;
        let mut scroll_offset = state.scroll_handle.offset();
        let mut cursor_bounds = None;

        // If the input has a fixed height (Otherwise is auto-grow), we need to add a bottom margin to the input.
        let top_bottom_margin = if state.mode.is_auto_grow() {
            line_height
        } else if visible_range.len() < BOTTOM_MARGIN_ROWS * 8 {
            line_height
        } else {
            BOTTOM_MARGIN_ROWS * line_height
        };

        // The cursor corresponds to the current cursor position in the text no only the line.
        let mut cursor_pos = None;
        let mut cursor_start = None;
        let mut cursor_end = None;

        let mut prev_lines_offset = 0;
        let mut offset_y = px(0.);
        for (ix, wrap_line) in text_wrapper.lines.iter().enumerate() {
            let row = ix;
            let line_origin = point(px(0.), offset_y);

            // break loop if all cursor positions are found
            if cursor_pos.is_some() && cursor_start.is_some() && cursor_end.is_some() {
                break;
            }

            let in_visible_range = ix >= visible_range.start;
            if let Some(line) = in_visible_range
                .then(|| lines.get(ix.saturating_sub(visible_range.start)))
                .flatten()
            {
                // If in visible range lines
                if cursor_pos.is_none() {
                    let offset = cursor.saturating_sub(prev_lines_offset);
                    if let Some(pos) = line.position_for_index(offset, last_layout) {
                        current_row = Some(row);
                        cursor_pos = Some(line_origin + pos);
                    }
                }
                if cursor_start.is_none() {
                    let offset = selected_range.start.saturating_sub(prev_lines_offset);
                    if let Some(pos) = line.position_for_index(offset, last_layout) {
                        cursor_start = Some(line_origin + pos);
                    }
                }
                if cursor_end.is_none() {
                    let offset = selected_range.end.saturating_sub(prev_lines_offset);
                    if let Some(pos) = line.position_for_index(offset, last_layout) {
                        cursor_end = Some(line_origin + pos);
                    }
                }

                offset_y += line.size(line_height).height;
                // +1 for the last `\n`
                prev_lines_offset += line.len() + 1;
            } else {
                // If not in the visible range.

                // Just increase the offset_y and prev_lines_offset.
                // This will let the scroll_offset to track the cursor position correctly.
                if prev_lines_offset >= cursor && cursor_pos.is_none() {
                    current_row = Some(row);
                    cursor_pos = Some(line_origin);
                }
                if prev_lines_offset >= selected_range.start && cursor_start.is_none() {
                    cursor_start = Some(line_origin);
                }
                if prev_lines_offset >= selected_range.end && cursor_end.is_none() {
                    cursor_end = Some(line_origin);
                }

                offset_y += wrap_line.height(line_height);
                // +1 for the last `\n`
                prev_lines_offset += wrap_line.len() + 1;
            }
        }

        if let (Some(cursor_pos), Some(cursor_start), Some(cursor_end)) =
            (cursor_pos, cursor_start, cursor_end)
        {
            let selection_changed = state.last_selected_range != Some(selected_range);
            if selection_changed && !is_selected_all {
                // Apart from left alignment, just leave enough space for the cursor size on the right side.
                let safety_margin = if last_layout.text_align == TextAlign::Left {
                    RIGHT_MARGIN
                } else {
                    CURSOR_WIDTH
                };

                scroll_offset.x = if scroll_offset.x + cursor_pos.x
                    > (bounds.size.width - line_number_width - safety_margin)
                {
                    // cursor is out of right
                    bounds.size.width - line_number_width - safety_margin - cursor_pos.x
                } else if scroll_offset.x + cursor_pos.x < px(0.) {
                    // cursor is out of left
                    scroll_offset.x - cursor_pos.x
                } else {
                    scroll_offset.x
                };

                // If we change the scroll_offset.y, GPUI will render and trigger the next run loop.
                // So, here we just adjust offset by `line_height` for move smooth.
                scroll_offset.y =
                    if scroll_offset.y + cursor_pos.y > bounds.size.height - top_bottom_margin {
                        // cursor is out of bottom
                        scroll_offset.y - line_height
                    } else if scroll_offset.y + cursor_pos.y < top_bottom_margin {
                        // cursor is out of top
                        (scroll_offset.y + line_height).min(px(0.))
                    } else {
                        scroll_offset.y
                    };

                // For selection to move scroll
                if state.selection_reversed {
                    if scroll_offset.x + cursor_start.x < px(0.) {
                        // selection start is out of left
                        scroll_offset.x = -cursor_start.x;
                    }
                    if scroll_offset.y + cursor_start.y < px(0.) {
                        // selection start is out of top
                        scroll_offset.y = -cursor_start.y;
                    }
                } else {
                    // TODO: Consider to remove this part,
                    // maybe is not necessary (But selection_reversed is needed).
                    if scroll_offset.x + cursor_end.x <= px(0.) {
                        // selection end is out of left
                        scroll_offset.x = -cursor_end.x;
                    }
                    if scroll_offset.y + cursor_end.y <= px(0.) {
                        // selection end is out of top
                        scroll_offset.y = -cursor_end.y;
                    }
                }
            }

            // cursor bounds
            let cursor_height = match state.size {
                crate::Size::Large => 1.,
                crate::Size::Small => 0.75,
                _ => 0.85,
            } * line_height;

            cursor_bounds = Some(Bounds::new(
                point(
                    bounds.left() + cursor_pos.x + line_number_width + scroll_offset.x,
                    bounds.top() + cursor_pos.y + ((line_height - cursor_height) / 2.),
                ),
                size(CURSOR_WIDTH, cursor_height),
            ));
        }

        if let Some(deferred_scroll_offset) = state.deferred_scroll_offset {
            scroll_offset = deferred_scroll_offset;
        }

        bounds.origin = bounds.origin + scroll_offset;

        (cursor_bounds, scroll_offset, current_row)
    }

    /// Layout the match range to a Path.
    pub(crate) fn layout_match_range(
        range: Range<usize>,
        last_layout: &LastLayout,
        bounds: &Bounds<Pixels>,
    ) -> Option<Path<Pixels>> {
        if range.is_empty() {
            return None;
        }

        if range.start < last_layout.visible_range_offset.start
            || range.end > last_layout.visible_range_offset.end
        {
            return None;
        }

        let line_height = last_layout.line_height;
        let visible_top = last_layout.visible_top;
        let visible_start_offset = last_layout.visible_range_offset.start;
        let lines = &last_layout.lines;
        let line_number_width = last_layout.line_number_width;

        let start_ix = range.start;
        let end_ix = range.end;

        let mut prev_lines_offset = visible_start_offset;
        let mut offset_y = visible_top;
        let mut line_corners = vec![];

        for line in lines.iter() {
            let line_size = line.size(line_height);
            let line_wrap_width = line_size.width;

            let line_origin = point(px(0.), offset_y);

            let line_cursor_start =
                line.position_for_index(start_ix.saturating_sub(prev_lines_offset), last_layout);
            let line_cursor_end =
                line.position_for_index(end_ix.saturating_sub(prev_lines_offset), last_layout);

            if line_cursor_start.is_some() || line_cursor_end.is_some() {
                let start = line_cursor_start
                    .unwrap_or_else(|| line.position_for_index(0, last_layout).unwrap());

                let end = line_cursor_end
                    .unwrap_or_else(|| line.position_for_index(line.len(), last_layout).unwrap());

                // Split the selection into multiple items
                let wrapped_lines =
                    (end.y / line_height).ceil() as usize - (start.y / line_height).ceil() as usize;

                let mut end_x = end.x;
                if wrapped_lines > 0 {
                    end_x = line_wrap_width;
                }

                // Ensure at least 6px width for the selection for empty lines.
                end_x = end_x.max(start.x + px(6.));

                line_corners.push(Corners {
                    top_left: line_origin + point(start.x, start.y),
                    top_right: line_origin + point(end_x, start.y),
                    bottom_left: line_origin + point(start.x, start.y + line_height),
                    bottom_right: line_origin + point(end_x, start.y + line_height),
                });

                // wrapped lines
                for i in 1..=wrapped_lines {
                    let start = point(px(0.), start.y + i as f32 * line_height);
                    let mut end = point(end.x, end.y + i as f32 * line_height);
                    if i < wrapped_lines {
                        end.x = line_size.width;
                    }

                    line_corners.push(Corners {
                        top_left: line_origin + point(start.x, start.y),
                        top_right: line_origin + point(end.x, start.y),
                        bottom_left: line_origin + point(start.x, start.y + line_height),
                        bottom_right: line_origin + point(end.x, start.y + line_height),
                    });
                }
            }

            if line_cursor_start.is_some() && line_cursor_end.is_some() {
                break;
            }

            offset_y += line_size.height;
            // +1 for skip the last `\n`
            prev_lines_offset += line.len() + 1;
        }

        let mut points = vec![];
        if line_corners.is_empty() {
            return None;
        }

        // Fix corners to make sure the left to right direction
        for corners in &mut line_corners {
            if corners.top_left.x > corners.top_right.x {
                std::mem::swap(&mut corners.top_left, &mut corners.top_right);
                std::mem::swap(&mut corners.bottom_left, &mut corners.bottom_right);
            }
        }

        for corners in &line_corners {
            points.push(corners.top_right);
            points.push(corners.bottom_right);
            points.push(corners.bottom_left);
        }

        let mut rev_line_corners = line_corners.iter().rev().peekable();
        while let Some(corners) = rev_line_corners.next() {
            points.push(corners.top_left);
            if let Some(next) = rev_line_corners.peek() {
                if next.top_left.x > corners.top_left.x {
                    points.push(point(next.top_left.x, corners.top_left.y));
                }
            }
        }

        // print_points_as_svg_path(&line_corners, &points);

        let path_origin = bounds.origin + point(line_number_width, px(0.));
        let first_p = *points.get(0).unwrap();
        let mut builder = gpui::PathBuilder::fill();
        builder.move_to(path_origin + first_p);
        for p in points.iter().skip(1) {
            builder.line_to(path_origin + *p);
        }

        builder.build().ok()
    }

    fn layout_search_matches(
        &self,
        last_layout: &LastLayout,
        bounds: &Bounds<Pixels>,
        cx: &mut App,
    ) -> Vec<(Path<Pixels>, bool)> {
        let search_panel = self.state.read(cx).search_panel.clone();
        let Some((ranges, current_match_ix)) = search_panel.and_then(|panel| {
            if let Some(matcher) = panel.read(cx).matcher() {
                Some((matcher.matched_ranges.clone(), matcher.current_match_ix))
            } else {
                None
            }
        }) else {
            return vec![];
        };

        let mut paths = Vec::new();
        for (index, range) in ranges.as_ref().iter().enumerate() {
            if let Some(path) = Self::layout_match_range(range.clone(), last_layout, bounds) {
                paths.push((path, current_match_ix == index));
            }
        }

        paths
    }

    fn layout_hover_highlight(
        &self,
        last_layout: &LastLayout,
        bounds: &Bounds<Pixels>,
        cx: &mut App,
    ) -> Option<Path<Pixels>> {
        let hover_popover = self.state.read(cx).hover_popover.clone();
        let Some(symbol_range) = hover_popover.map(|popover| popover.read(cx).symbol_range.clone())
        else {
            return None;
        };

        Self::layout_match_range(symbol_range, last_layout, bounds)
    }

    fn layout_document_colors(
        &self,
        document_colors: &[(Range<usize>, Hsla)],
        last_layout: &LastLayout,
        bounds: &Bounds<Pixels>,
    ) -> Vec<(Path<Pixels>, Hsla)> {
        let mut paths = vec![];
        for (range, color) in document_colors.iter() {
            if let Some(path) = Self::layout_match_range(range.clone(), last_layout, bounds) {
                paths.push((path, *color));
            }
        }

        paths
    }

    fn selection_bg_segments(
        state: &InputState,
        last_layout: &LastLayout,
        selection_color: Hsla,
        window: &mut Window,
    ) -> Vec<(Range<usize>, Hsla)> {
        if !state.focus_handle.is_focused(window) {
            return Vec::new();
        }

        let mut selected_range = state.selected_range;
        if let Some(ime_marked_range) = &state.ime_marked_range {
            if !ime_marked_range.is_empty() {
                selected_range = (ime_marked_range.end..ime_marked_range.end).into();
            }
        }
        if selected_range.is_empty() {
            return Vec::new();
        }

        if state.masked {
            // Because masked use `*`, 1 char with 1 byte.
            selected_range.start = state.text.offset_to_char_index(selected_range.start);
            selected_range.end = state.text.offset_to_char_index(selected_range.end);
        }

        let (start_ix, end_ix) = if selected_range.start < selected_range.end {
            (selected_range.start, selected_range.end)
        } else {
            (selected_range.end, selected_range.start)
        };

        let range = start_ix.max(last_layout.visible_range_offset.start)
            ..end_ix.min(last_layout.visible_range_offset.end);

        if range.is_empty() {
            return Vec::new();
        }

        vec![(range, selection_color)]
    }

    fn layout_selections(
        &self,
        last_layout: &LastLayout,
        bounds: &mut Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Path<Pixels>> {
        let state = self.state.read(cx);
        if !state.focus_handle.is_focused(window) {
            return None;
        }

        let mut selected_range = state.selected_range;
        if let Some(ime_marked_range) = &state.ime_marked_range {
            if !ime_marked_range.is_empty() {
                selected_range = (ime_marked_range.end..ime_marked_range.end).into();
            }
        }
        if selected_range.is_empty() {
            return None;
        }

        if state.masked {
            // Because masked use `*`, 1 char with 1 byte.
            selected_range.start = state.text.offset_to_char_index(selected_range.start);
            selected_range.end = state.text.offset_to_char_index(selected_range.end);
        }

        let (start_ix, end_ix) = if selected_range.start < selected_range.end {
            (selected_range.start, selected_range.end)
        } else {
            (selected_range.end, selected_range.start)
        };

        let range = start_ix.max(last_layout.visible_range_offset.start)
            ..end_ix.min(last_layout.visible_range_offset.end);

        Self::layout_match_range(range, &last_layout, bounds)
    }

    /// Calculate the visible range of lines in the viewport.
    ///
    /// Returns
    ///
    /// - visible_range: The visible range is based on unwrapped lines (Zero based).
    /// - visible_top: The top position of the first visible line in the scroll viewport.
    fn calculate_visible_range(
        &self,
        state: &InputState,
        line_height: Pixels,
        input_height: Pixels,
    ) -> (Range<usize>, Pixels) {
        // Add extra rows to avoid showing empty space when scroll to bottom.
        let extra_rows = 1;
        let mut visible_top = px(0.);
        if state.mode.is_single_line() {
            return (0..1, visible_top);
        }

        let total_lines = state.text_wrapper.len();
        let scroll_top = if let Some(deferred_scroll_offset) = state.deferred_scroll_offset {
            deferred_scroll_offset.y
        } else {
            state.scroll_handle.offset().y
        };

        let mut visible_range = 0..total_lines;
        let mut line_bottom = px(0.);
        for (ix, line) in state.text_wrapper.lines.iter().enumerate() {
            let wrapped_height = line.height(line_height);
            line_bottom += wrapped_height;

            if line_bottom < -scroll_top {
                visible_top = line_bottom - wrapped_height;
                visible_range.start = ix;
            }

            if line_bottom + scroll_top >= input_height {
                visible_range.end = (ix + extra_rows).min(total_lines);
                break;
            }
        }

        (visible_range, visible_top)
    }

    /// Return (line_number_width, line_number_len)
    fn layout_line_numbers(
        state: &InputState,
        text: &Rope,
        font_size: Pixels,
        style: &TextStyle,
        window: &mut Window,
    ) -> (Pixels, usize) {
        let total_lines = text.lines_len();
        let line_number_len = match total_lines {
            0..=9999 => 5,
            10000..=99999 => 6,
            100000..=999999 => 7,
            _ => 8,
        };

        let line_number_width = if state.mode.line_number() {
            let empty_line_number = window.text_system().shape_line(
                "+".repeat(line_number_len).into(),
                font_size,
                &[TextRun {
                    len: line_number_len,
                    font: style.font(),
                    color: gpui::black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );

            empty_line_number.width + px(6.) + LINE_NUMBER_RIGHT_MARGIN
        } else {
            px(0.)
        };

        (line_number_width, line_number_len)
    }

    /// Layout shaped lines for whitespace indicators (space and tab).
    ///
    /// Returns `WhitespaceIndicators` with shaped lines for space and tab characters.
    fn layout_whitespace_indicators(
        state: &InputState,
        text_size: Pixels,
        style: &TextStyle,
        window: &mut Window,
        cx: &App,
    ) -> Option<WhitespaceIndicators> {
        if !state.show_whitespaces {
            return None;
        }

        let invisible_color = cx
            .theme()
            .highlight_theme
            .style
            .editor_invisible
            .unwrap_or(cx.theme().muted_foreground);

        let space_font_size = text_size.half();
        let tab_font_size = text_size;

        let space_text = SharedString::new_static("•");
        let space = window.text_system().shape_line(
            space_text.clone(),
            space_font_size,
            &[TextRun {
                len: space_text.len(),
                font: style.font(),
                color: invisible_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );

        let tab_text = SharedString::new_static("→");
        let tab = window.text_system().shape_line(
            tab_text.clone(),
            tab_font_size,
            &[TextRun {
                len: tab_text.len(),
                font: style.font(),
                color: invisible_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );

        Some(WhitespaceIndicators { space, tab })
    }

    /// Compute inline completion ghost lines for rendering.
    ///
    /// Returns (first_line, ghost_lines) where:
    /// - first_line: Shaped text for the first line (goes after cursor on same line)
    /// - ghost_lines: Shaped lines for subsequent lines (shift content down)
    fn layout_inline_completion(
        state: &InputState,
        visible_range: &Range<usize>,
        font_size: Pixels,
        window: &mut Window,
        cx: &App,
    ) -> (Option<ShapedLine>, Vec<ShapedLine>) {
        // Must be focused to show inline completion
        if !state.focus_handle.is_focused(window) {
            return (None, vec![]);
        }

        let Some(completion_item) = state.inline_completion.item.as_ref() else {
            return (None, vec![]);
        };

        // Get cursor row from cursor position
        let cursor_row = state.cursor_position().line as usize;

        // Only show if cursor row is visible
        if cursor_row < visible_range.start || cursor_row >= visible_range.end {
            return (None, vec![]);
        }

        let completion_text = &completion_item.insert_text;
        let completion_color = cx.theme().muted_foreground.opacity(0.5);

        let text_style = window.text_style();
        let font = text_style.font();

        let lines: Vec<&str> = completion_text.split('\n').collect();
        if lines.is_empty() {
            return (None, vec![]);
        }

        // Shape first line (goes after cursor)
        let first_text: SharedString = lines[0].to_string().into();
        let first_line = if !first_text.is_empty() {
            let first_run = TextRun {
                len: first_text.len(),
                font: font.clone(),
                color: completion_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            Some(
                window
                    .text_system()
                    .shape_line(first_text, font_size, &[first_run], None),
            )
        } else {
            None
        };

        // Shape ghost lines (lines 2+ that shift content down)
        let ghost_lines: Vec<ShapedLine> = lines[1..]
            .iter()
            .map(|line_text| {
                let text: SharedString = line_text.to_string().into();
                let len = text.len().max(1); // Ensure at least 1 for empty lines
                let run = TextRun {
                    len,
                    font: font.clone(),
                    color: completion_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                // Use space for empty lines so they take up height
                let shaped_text = if text.is_empty() { " ".into() } else { text };
                window
                    .text_system()
                    .shape_line(shaped_text, font_size, &[run], None)
            })
            .collect();

        (first_line, ghost_lines)
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_lines(
        state: &InputState,
        display_text: &Rope,
        last_layout: &LastLayout,
        font_size: Pixels,
        runs: &[TextRun],
        document_bg_segments: &[(Range<usize>, Hsla)],
        selection_bg_segments: &[(Range<usize>, Hsla)],
        whitespace_indicators: Option<WhitespaceIndicators>,
        window: &mut Window,
    ) -> Vec<LineLayout> {
        let is_single_line = state.mode.is_single_line();
        let text_wrapper = &state.text_wrapper;
        let visible_range = &last_layout.visible_range;
        let visible_range_offset = &last_layout.visible_range_offset;

        if is_single_line {
            let shaped_line = window.text_system().shape_line(
                display_text.to_string().into(),
                font_size,
                &runs,
                None,
            );

            let line_layout = LineLayout::new()
                .lines(smallvec::smallvec![shaped_line])
                .with_whitespaces(whitespace_indicators);
            return vec![line_layout];
        }

        // Empty to use placeholder, the placeholder is not in the text_wrapper map.
        if state.text.len() == 0 {
            return display_text
                .to_string()
                .split("\n")
                .map(|line| {
                    let shaped_line = window.text_system().shape_line(
                        line.to_string().into(),
                        font_size,
                        &runs,
                        None,
                    );
                    LineLayout::new()
                        .lines(smallvec::smallvec![shaped_line])
                        .with_whitespaces(whitespace_indicators.clone())
                })
                .collect();
        }

        let visible_text = display_text
            .slice_lines(visible_range.start..visible_range.end)
            .to_string();

        let mut lines = vec![];
        let mut offset = 0;
        for (ix, line) in visible_text.split("\n").enumerate() {
            let line_item = text_wrapper
                .lines
                .get(visible_range.start + ix)
                .expect("line should exists in text_wrapper");

            debug_assert_eq!(line_item.len(), line.len());

            let mut wrapped_lines = SmallVec::with_capacity(1);

            // Wrap range 安全切：start 用 floor（防与上一行重复），end 用 ceil（防吞字符）。
            // 例：'名' bytes 22..25，wrap 切在 byte 24：
            //   start:  floor(0)=0
            //   end:    ceil(24)=25  ← 包含完整 '名' 字符
            // GPUI LineWrapper 按 byte 算 wrap 时 range 落在 CJK 字符中间会 panic，
            // 因此两端必须 char-safe；这里 start=floor + end=ceil 是 char-safe 的同时
            // 不丢字也不重复（start 是上一 wrap 行的 end floor→同一字符起点 floor→一致）。
            let snapped: Vec<Range<usize>> = line_item
                .wrapped_lines
                .iter()
                .map(|r| {
                    let start = floor_char_boundary(line, r.start);
                    let end = if r.end >= line.len() {
                        line.len()
                    } else {
                        ceil_char_boundary(line, r.end)
                    };
                    start..end.max(start)
                })
                .collect();

            for range in &snapped {
                let line_runs = runs_for_range(line, runs, offset, range);
                let line_runs = if document_bg_segments.is_empty() {
                    line_runs
                } else {
                    split_runs_by_bg_segments(
                        line,
                        visible_range_offset.start + offset,
                        &line_runs,
                        document_bg_segments,
                    )
                };
                let line_runs = if selection_bg_segments.is_empty() {
                    line_runs
                } else {
                    split_runs_by_bg_segments(
                        line,
                        visible_range_offset.start + offset,
                        &line_runs,
                        selection_bg_segments,
                    )
                };

                // 同向 floor clip 已经把 range 钳到字符边界，直接切片安全
                let sub_line: SharedString = line[range.clone()].to_string().into();
                // 终末防御：累加每个 run.len，如果落在 sub_line 字符中间，向前 floor
                // （之前所有 layer 都已 clip，这里是兜底，防止任何遗漏的 byte↔char 错位）
                let line_runs = char_safe_runs(&sub_line, line_runs);
                let shaped_line = window
                    .text_system()
                    .shape_line(sub_line, font_size, &line_runs, None);

                wrapped_lines.push(shaped_line);
            }

            let line_layout = LineLayout::new()
                .lines(wrapped_lines)
                .with_whitespaces(whitespace_indicators.clone());
            lines.push(line_layout);

            // +1 for the `\n`
            offset += line.len() + 1;
        }

        lines
    }

    /// First usize is the offset of skipped.
    fn highlight_lines(
        &mut self,
        visible_range: &Range<usize>,
        _visible_top: Pixels,
        visible_byte_range: Range<usize>,
        cx: &mut App,
    ) -> Option<Vec<(Range<usize>, HighlightStyle)>> {
        let state = self.state.read(cx);
        let text = &state.text;
        let is_multi_line = state.mode.is_multi_line();

        let (mut highlighter, diagnostics) = match &state.mode {
            InputMode::CodeEditor {
                highlighter,
                diagnostics,
                ..
            } => (highlighter.borrow_mut(), diagnostics),
            _ => return None,
        };
        let highlighter = highlighter.as_mut()?;

        let mut styles = vec![];
        let visible_buffer_lines: Vec<usize> = (visible_range.start..visible_range.end).collect();

        let flush_range = |start_line: usize, end_line: usize, skip: bool, styles: &mut Vec<_>| {
            let byte_start = text.line_start_offset(start_line);
            let byte_end = if is_multi_line {
                text.line_start_offset(end_line + 1)
            } else {
                text.line_end_offset(end_line)
            };
            let range_styles = if skip {
                vec![(byte_start..byte_end, HighlightStyle::default())]
            } else {
                highlighter.styles(&(byte_start..byte_end), &cx.theme().highlight_theme)
            };

            *styles = gpui::combine_highlights(styles.clone(), range_styles).collect();
        };

        let mut visible_iter = visible_buffer_lines.iter().peekable();
        let mut range_start: Option<usize> = None;

        while let Some(&line) = visible_iter.next() {
            let line_len = text.slice_line(line).len();
            if line_len > MAX_HIGHLIGHT_LINE_LENGTH {
                if let Some(start) = range_start.take() {
                    flush_range(start, line - 1, false, &mut styles);
                }
                flush_range(line, line, true, &mut styles);
                continue;
            }

            range_start.get_or_insert(line);
            if visible_iter
                .peek()
                .map(|&&next| next == line + 1)
                .unwrap_or(false)
            {
                continue;
            }

            let start_line = range_start.take().unwrap();
            flush_range(start_line, line, false, &mut styles);
        }

        let diagnostic_styles = diagnostics.styles_for_range(&visible_byte_range, cx);

        // hover definition style
        if let Some(hover_style) = self.layout_hover_definition(cx) {
            styles.push(hover_style);
        }

        // Combine marker styles
        styles = gpui::combine_highlights(diagnostic_styles, styles).collect();

        Some(styles)
    }
}

pub(super) struct PrepaintState {
    /// The lines of entire lines.
    last_layout: LastLayout,
    /// The lines only contains the visible lines in the viewport, based on `visible_range`.
    ///
    /// The child is the soft lines.
    line_numbers: Option<Vec<SmallVec<[ShapedLine; 1]>>>,
    /// Size of the scrollable area by entire lines.
    scroll_size: Size<Pixels>,
    cursor_bounds: Option<Bounds<Pixels>>,
    cursor_scroll_offset: Point<Pixels>,
    /// row index (zero based), no wrap, same line as the cursor.
    current_row: Option<usize>,
    selection_path: Option<Path<Pixels>>,
    hover_highlight_path: Option<Path<Pixels>>,
    search_match_paths: Vec<(Path<Pixels>, bool)>,
    document_color_paths: Vec<(Path<Pixels>, Hsla)>,
    hover_definition_hitbox: Option<Hitbox>,
    indent_guides_path: Option<Path<Pixels>>,
    bounds: Bounds<Pixels>,
    // Inline completion rendering data
    /// Shaped ghost lines to paint after cursor row (completion lines 2+)
    ghost_lines: Vec<ShapedLine>,
    /// First line of inline completion (painted after cursor on same line)
    ghost_first_line: Option<ShapedLine>,
    ghost_lines_height: Pixels,
}

impl PrepaintState {
    /// Returns cursor bounds adjusted for scroll offset, if available.
    fn cursor_bounds_with_scroll(&self) -> Option<Bounds<Pixels>> {
        self.cursor_bounds.map(|mut bounds| {
            bounds.origin.y += self.cursor_scroll_offset.y;
            bounds
        })
    }
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// A debug function to print points as SVG path.
#[allow(unused)]
fn print_points_as_svg_path(
    line_corners: &Vec<Corners<Point<Pixels>>>,
    points: &Vec<Point<Pixels>>,
) {
    for corners in line_corners {
        println!(
            "tl: ({}, {}), tr: ({}, {}), bl: ({}, {}), br: ({}, {})",
            corners.top_left.x.as_f32() as i32,
            corners.top_left.y.as_f32() as i32,
            corners.top_right.x.as_f32() as i32,
            corners.top_right.y.as_f32() as i32,
            corners.bottom_left.x.as_f32() as i32,
            corners.bottom_left.y.as_f32() as i32,
            corners.bottom_right.x.as_f32() as i32,
            corners.bottom_right.y.as_f32() as i32,
        );
    }

    if points.len() > 0 {
        println!(
            "M{},{}",
            points[0].x.as_f32() as i32,
            points[0].y.as_f32() as i32
        );
        for p in points.iter().skip(1) {
            println!("L{},{}", p.x.as_f32() as i32, p.y.as_f32() as i32);
        }
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let state = self.state.read(cx);
        let line_height = window.line_height();

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        if state.mode.is_multi_line() {
            style.flex_grow = 1.0;
            style.size.height = relative(1.).into();
            if state.mode.is_auto_grow() {
                // Auto grow to let height match to rows, but not exceed max rows.
                let rows = state.mode.max_rows().min(state.mode.rows());
                style.min_size.height = (rows * line_height).into();
            } else {
                style.min_size.height = line_height.into();
            }
        } else {
            // For single-line inputs, the minimum height should be the line height
            style.size.height = line_height.into();
        };

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let style = window.text_style();
        let font = style.font();
        let text_size = style.font_size.to_pixels(window.rem_size());

        self.state.update(cx, |state, cx| {
            state.text_wrapper.set_font(font, text_size, cx);
            state.text_wrapper.prepare_if_need(&state.text, cx);
        });

        let state = self.state.read(cx);
        let line_height = window.line_height();

        let (visible_range, visible_top) =
            self.calculate_visible_range(&state, line_height, bounds.size.height);
        let visible_start_offset = state.text.line_start_offset(visible_range.start);
        let visible_end_offset = state
            .text
            .line_end_offset(visible_range.end.saturating_sub(1));

        let highlight_styles = self.highlight_lines(
            &visible_range,
            visible_top,
            visible_start_offset..visible_end_offset,
            cx,
        );
        let selection_color = cx.theme().selection;

        let state = self.state.read(cx);
        let multi_line = state.mode.is_multi_line();
        let text = state.text.clone();
        let is_empty = text.len() == 0;
        let placeholder = self.placeholder.clone();

        let mut bounds = bounds;

        let (display_text, text_color) = if is_empty {
            (
                &Rope::from(placeholder.as_str()),
                cx.theme().muted_foreground,
            )
        } else if state.masked {
            (
                &Rope::from("*".repeat(text.chars().count())),
                cx.theme().foreground,
            )
        } else {
            (&text, cx.theme().foreground)
        };

        let text_style = window.text_style();

        // Calculate the width of the line numbers
        let (line_number_width, line_number_len) =
            Self::layout_line_numbers(&state, &text, text_size, &text_style, window);

        let wrap_width = if multi_line && state.soft_wrap {
            Some(bounds.size.width - line_number_width - RIGHT_MARGIN)
        } else {
            None
        };

        let mut last_layout = LastLayout {
            visible_range,
            visible_top,
            visible_range_offset: visible_start_offset..visible_end_offset,
            line_height,
            wrap_width,
            line_number_width,
            lines: Rc::new(vec![]),
            cursor_bounds: None,
            text_align: state.text_align,
            content_width: bounds.size.width,
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let marked_run = TextRun {
            len: 0,
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: Some(UnderlineStyle {
                thickness: px(1.),
                color: Some(text_color),
                wavy: false,
            }),
            strikethrough: None,
        };

        let runs = if !is_empty {
            if let Some(highlight_styles) = highlight_styles {
                let mut runs = vec![];

                runs.extend(highlight_styles.iter().map(|(range, style)| {
                    let mut run = text_style.clone().highlight(*style).to_run(range.len());
                    if let Some(ime_marked_range) = &state.ime_marked_range {
                        if range.start >= ime_marked_range.start
                            && range.end <= ime_marked_range.end
                        {
                            run.color = marked_run.color;
                            run.strikethrough = marked_run.strikethrough;
                            run.underline = marked_run.underline;
                        }
                    }

                    run
                }));

                runs.into_iter().filter(|run| run.len > 0).collect()
            } else {
                vec![run]
            }
        } else if let Some(ime_marked_range) = &state.ime_marked_range {
            // IME marked text
            vec![
                TextRun {
                    len: ime_marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: ime_marked_range.end - ime_marked_range.start,
                    underline: marked_run.underline,
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - ime_marked_range.end,
                    ..run.clone()
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let document_colors = state
            .lsp
            .document_colors_for_range(&text, &last_layout.visible_range);
        let selection_bg_segments =
            Self::selection_bg_segments(&state, &last_layout, selection_color, window);

        // Create shaped lines for whitespace indicators before layout
        let whitespace_indicators =
            Self::layout_whitespace_indicators(&state, text_size, &text_style, window, cx);

        let lines = Self::layout_lines(
            &state,
            &display_text,
            &last_layout,
            text_size,
            &runs,
            &document_colors,
            &selection_bg_segments,
            whitespace_indicators,
            window,
        );

        let mut longest_line_width = wrap_width.unwrap_or(px(0.));
        // 1. Single line
        // 2. Multi-line with soft wrap disabled.
        if state.mode.is_single_line() || !state.soft_wrap {
            let longest_row = state.text_wrapper.longest_row.row;
            let longest_line: SharedString = state.text.slice_line(longest_row).to_string().into();
            longest_line_width = window
                .text_system()
                .shape_line(
                    longest_line.clone(),
                    text_size,
                    &[TextRun {
                        len: longest_line.len(),
                        font: style.font(),
                        color: gpui::black(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    wrap_width,
                )
                .width;
        }
        last_layout.lines = Rc::new(lines);

        let (ghost_first_line, ghost_lines) = Self::layout_inline_completion(
            state,
            &last_layout.visible_range,
            text_size,
            window,
            cx,
        );
        let ghost_line_count = ghost_lines.len();
        let ghost_lines_height = ghost_line_count as f32 * line_height;

        let total_wrapped_lines = state.text_wrapper.len();
        let empty_bottom_height = if state.mode.is_code_editor() {
            bounds
                .size
                .height
                .half()
                .max(BOTTOM_MARGIN_ROWS * line_height)
        } else {
            px(0.)
        };

        let mut scroll_size = size(
            if longest_line_width + line_number_width + RIGHT_MARGIN > bounds.size.width {
                longest_line_width + line_number_width + RIGHT_MARGIN
            } else {
                longest_line_width
            },
            (total_wrapped_lines as f32 * line_height + empty_bottom_height + ghost_lines_height)
                .max(bounds.size.height),
        );

        // TODO: should be add some gap to right, to convenient to focus on boundary position
        if last_layout.text_align == TextAlign::Right || last_layout.text_align == TextAlign::Center
        {
            scroll_size.width = longest_line_width + line_number_width;
        }

        // `position_for_index` for example
        //
        // #### text
        //
        // Hello 世界，this is GPUI component.
        // The GPUI Component is a collection of UI components for
        // GPUI framework, including Button, Input, Checkbox, Radio,
        // Dropdown, Tab, and more...
        //
        // wrap_width: 444px, line_height: 20px
        //
        // #### lines[0]
        //
        // | index | pos              | line |
        // |-------|------------------|------|
        // | 5     | (37 px, 0.0)     | 0    |
        // | 38    | (261.7 px, 20.0) | 0    |
        // | 40    | None             | -    |
        //
        // #### lines[1]
        //
        // | index | position              | line |
        // |-------|-----------------------|------|
        // | 5     | (43.578125 px, 0.0)   | 0    |
        // | 56    | (422.21094 px, 0.0)   | 0    |
        // | 57    | (11.6328125 px, 20.0) | 1    |
        // | 114   | (429.85938 px, 20.0)  | 1    |
        // | 115   | (11.3125 px, 40.0)    | 2    |

        // Calculate the scroll offset to keep the cursor in view

        let (cursor_bounds, cursor_scroll_offset, current_row) =
            self.layout_cursor(&last_layout, &mut bounds, window, cx);
        last_layout.cursor_bounds = cursor_bounds;

        let search_match_paths = self.layout_search_matches(&last_layout, &mut bounds, cx);
        let selection_path = self.layout_selections(&last_layout, &mut bounds, window, cx);
        let hover_highlight_path = self.layout_hover_highlight(&last_layout, &mut bounds, cx);
        let document_color_paths =
            self.layout_document_colors(&document_colors, &last_layout, &bounds);

        let state = self.state.read(cx);
        let line_numbers = if state.mode.line_number() {
            let mut line_numbers = vec![];
            let other_line_runs = vec![TextRun {
                len: line_number_len,
                font: style.font(),
                color: cx.theme().muted_foreground,
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            let current_line_runs = vec![TextRun {
                len: line_number_len,
                font: style.font(),
                color: cx.theme().foreground,
                background_color: None,
                underline: None,
                strikethrough: None,
            }];

            // build line numbers
            for (ix, line) in last_layout.lines.iter().enumerate() {
                let ix = last_layout.visible_range.start + ix;
                let line_no = format!("{:>width$}", ix + 1, width = line_number_len).into();

                let runs = if current_row == Some(ix) {
                    &current_line_runs
                } else {
                    &other_line_runs
                };

                let mut sub_lines: SmallVec<[ShapedLine; 1]> = SmallVec::new();
                sub_lines.push(
                    window
                        .text_system()
                        .shape_line(line_no, text_size, &runs, None),
                );
                for _ in 0..line.wrapped_lines.len().saturating_sub(1) {
                    sub_lines.push(ShapedLine::default());
                }
                line_numbers.push(sub_lines);
            }
            Some(line_numbers)
        } else {
            None
        };

        let hover_definition_hitbox = self.layout_hover_definition_hitbox(state, window, cx);
        let indent_guides_path =
            self.layout_indent_guides(state, &bounds, &last_layout, &text_style, window);

        PrepaintState {
            bounds,
            last_layout,
            scroll_size,
            line_numbers,
            cursor_bounds,
            cursor_scroll_offset,
            current_row,
            selection_path,
            search_match_paths,
            hover_highlight_path,
            hover_definition_hitbox,
            document_color_paths,
            indent_guides_path,
            ghost_first_line,
            ghost_lines,
            ghost_lines_height,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        input_bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.state.read(cx).focus_handle.clone();
        let show_cursor = self.state.read(cx).show_cursor(window, cx);
        let focused = focus_handle.is_focused(window);
        let bounds = prepaint.bounds;
        let selected_range = self.state.read(cx).selected_range;
        let visible_range = &prepaint.last_layout.visible_range;
        let text_align = prepaint.last_layout.text_align;

        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.state.clone()),
            cx,
        );

        // Set Root focused_input when self is focused
        if focused {
            let state = self.state.clone();
            if Root::read(window, cx).focused_input.as_ref() != Some(&state) {
                Root::update(window, cx, |root, _, cx| {
                    root.focused_input = Some(state);
                    cx.notify();
                });
            }
        }

        // And reset focused_input when next_frame start
        window.on_next_frame({
            let state = self.state.clone();
            move |window, cx| {
                if !focused && Root::read(window, cx).focused_input.as_ref() == Some(&state) {
                    Root::update(window, cx, |root, _, cx| {
                        root.focused_input = None;
                        cx.notify();
                    });
                }
            }
        });

        // Paint multi line text
        let line_height = window.line_height();
        let origin = bounds.origin;

        let invisible_top_padding = prepaint.last_layout.visible_top;

        let mut mask_offset_y = px(0.);
        let state = self.state.read(cx);
        if state.masked && state.text.len() > 0 {
            // Move down offset for vertical centering the *****
            if cfg!(target_os = "macos") {
                mask_offset_y = px(3.);
            } else {
                mask_offset_y = px(2.5);
            }
        }
        let active_line_color = cx.theme().highlight_theme.style.editor_active_line;

        // Paint active line
        let mut offset_y = px(0.);
        if let Some(line_numbers) = prepaint.line_numbers.as_ref() {
            offset_y += invisible_top_padding;

            // Each item is the normal lines.
            for (ix, lines) in line_numbers.iter().enumerate() {
                let row = visible_range.start + ix;
                let is_active = prepaint.current_row == Some(row);
                let p = point(input_bounds.origin.x, origin.y + offset_y);
                let height = line_height * lines.len() as f32;
                // Paint the current line background
                if is_active {
                    if let Some(bg_color) = active_line_color {
                        window.paint_quad(fill(
                            Bounds::new(p, size(bounds.size.width, height)),
                            bg_color,
                        ));
                    }
                }
                offset_y += height;
            }
        }

        // Paint indent guides
        if let Some(path) = prepaint.indent_guides_path.take() {
            window.paint_path(path, cx.theme().border.opacity(0.85));
        }

        // Paint selections
        if window.is_window_active() {
            let secondary_selection = cx.theme().selection.saturation(0.1);
            for (path, is_active) in prepaint.search_match_paths.iter() {
                window.paint_path(path.clone(), secondary_selection);

                if *is_active {
                    window.paint_path(path.clone(), cx.theme().selection);
                }
            }

            if let Some(path) = prepaint.selection_path.take() {
                window.paint_path(path, cx.theme().selection);
            }

            // Paint hover highlight
            if let Some(path) = prepaint.hover_highlight_path.take() {
                window.paint_path(path, secondary_selection);
            }
        }

        // Paint document colors
        for (path, color) in prepaint.document_color_paths.iter() {
            window.paint_path(path.clone(), *color);
        }

        // Paint text with inline completion ghost line support
        let mut offset_y = mask_offset_y + invisible_top_padding;
        let ghost_lines = &prepaint.ghost_lines;
        let has_ghost_lines = !ghost_lines.is_empty();

        // Keep scrollbar offset always be positive，Start from the left position
        let scroll_offset = if text_align == TextAlign::Right {
            (prepaint.scroll_size.width - prepaint.bounds.size.width).max(px(0.))
        } else if text_align == TextAlign::Center {
            (prepaint.scroll_size.width - prepaint.bounds.size.width)
                .half()
                .max(px(0.))
        } else {
            px(0.)
        };

        // Track the y-position of the cursor row for positioning the first line suffix
        let mut cursor_row_y = None;

        for (ix, line) in prepaint.last_layout.lines.iter().enumerate() {
            let row = visible_range.start + ix;
            let line_y = origin.y + offset_y;
            let p = point(
                origin.x + prepaint.last_layout.line_number_width + (scroll_offset),
                line_y,
            );

            // Paint the actual line
            _ = line.paint(
                p,
                line_height,
                text_align,
                Some(prepaint.last_layout.content_width),
                window,
                cx,
            );
            offset_y += line.size(line_height).height;

            if Some(row) == prepaint.current_row {
                cursor_row_y = Some(line_y);
            }

            // After the cursor row, paint ghost lines (which shifts subsequent content down)
            if has_ghost_lines && Some(row) == prepaint.current_row {
                let ghost_x = origin.x + prepaint.last_layout.line_number_width;

                for ghost_line in ghost_lines {
                    let ghost_p = point(ghost_x, origin.y + offset_y);

                    // Paint semi-transparent background for ghost line
                    let ghost_bounds = Bounds::new(
                        ghost_p,
                        size(
                            bounds.size.width - prepaint.last_layout.line_number_width,
                            line_height,
                        ),
                    );
                    window.paint_quad(fill(ghost_bounds, cx.theme().editor_background()));

                    // Paint ghost line text
                    _ = ghost_line.paint(
                        ghost_p,
                        line_height,
                        text_align,
                        Some(prepaint.last_layout.content_width),
                        window,
                        cx,
                    );
                    offset_y += line_height;
                }
            }
        }

        // Paint blinking cursor
        if focused && show_cursor {
            if let Some(cursor_bounds) = prepaint.cursor_bounds_with_scroll() {
                window.paint_quad(fill(cursor_bounds, cx.theme().primary));
            }
        }

        // Paint line numbers
        let mut offset_y = px(0.);
        if let Some(line_numbers) = prepaint.line_numbers.as_ref() {
            offset_y += invisible_top_padding;

            window.paint_quad(fill(
                Bounds {
                    origin: input_bounds.origin,
                    size: size(
                        prepaint.last_layout.line_number_width - LINE_NUMBER_RIGHT_MARGIN,
                        input_bounds.size.height + prepaint.ghost_lines_height,
                    ),
                },
                cx.theme().editor_background(),
            ));

            // Each item is the normal lines.
            for (ix, lines) in line_numbers.iter().enumerate() {
                let row = visible_range.start + ix;

                let p = point(input_bounds.origin.x, origin.y + offset_y);
                let is_active = prepaint.current_row == Some(row);

                let height = line_height * lines.len() as f32;
                // paint active line number background
                if is_active {
                    if let Some(bg_color) = active_line_color {
                        window.paint_quad(fill(
                            Bounds::new(p, size(prepaint.last_layout.line_number_width, height)),
                            bg_color,
                        ));
                    }
                }

                for line in lines {
                    _ = line.paint(p, line_height, TextAlign::Left, None, window, cx);
                    offset_y += line_height;
                }

                // Add ghost line height after cursor row for line numbers alignment
                if !prepaint.ghost_lines.is_empty() && prepaint.current_row == Some(row) {
                    offset_y += prepaint.ghost_lines_height;
                }
            }
        }

        self.state.update(cx, |state, cx| {
            state.last_layout = Some(prepaint.last_layout.clone());
            state.last_bounds = Some(bounds);
            state.last_cursor = Some(state.cursor());
            state.set_input_bounds(input_bounds, cx);
            state.last_selected_range = Some(selected_range);
            state.scroll_size = prepaint.scroll_size;
            state.update_scroll_offset(Some(prepaint.cursor_scroll_offset), cx);
            state.deferred_scroll_offset = None;

            cx.notify();
        });

        if let Some(hitbox) = prepaint.hover_definition_hitbox.as_ref() {
            window.set_cursor_style(gpui::CursorStyle::PointingHand, &hitbox);
        }

        // Paint inline completion first line suffix (after cursor on same line)
        if focused {
            if let Some(first_line) = &prepaint.ghost_first_line {
                if let (Some(cursor_bounds), Some(cursor_row_y)) =
                    (prepaint.cursor_bounds_with_scroll(), cursor_row_y)
                {
                    let first_line_x = cursor_bounds.origin.x + cursor_bounds.size.width;
                    let p = point(first_line_x, cursor_row_y);

                    // Paint background to cover any existing text
                    let bg_bounds = Bounds::new(p, size(first_line.width + px(4.), line_height));
                    window.paint_quad(fill(bg_bounds, cx.theme().editor_background()));

                    // Paint first line completion text
                    _ = first_line.paint(p, line_height, text_align, None, window, cx);
                }
            }
        }

        self.paint_mouse_listeners(window, cx);
    }
}

/// Get the runs for the given range.
///
/// `line` 用于把每个 run.len 对齐到字符边界：tree-sitter / IME 的 run 边界
/// 按 byte 计，可能落在 CJK 等多字节字符中间。GPUI 的 `shape_line` 内部
/// 走 `str::split_at(run.len)`，要求 `run.len` 必须是 `line` 的字符边界，
/// 否则 `end byte index N is not a char boundary` panic。
///
/// 切策略：start 用 floor（与上一 run 接续），end 用 ceil（避免吞完整 CJK 字符）。
/// 副作用：相邻 run 可能在同一 CJK 字符中间各持一段，但下一 run 起点 floor
/// 会与本 run 终点 ceil 重合在该字符起点 → 仍然 char-safe，且不丢字。
pub(super) fn runs_for_range(
    line: &str,
    runs: &[TextRun],
    line_offset: usize,
    range: &Range<usize>,
) -> Vec<TextRun> {
    let mut result = vec![];
    let range = (line_offset + range.start)..(line_offset + range.end);
    let mut cursor = 0;
    // 上一 run 在 line 内的 safe_end（绝对字节 = line_offset + 本地）。
    // 用它保证下一 run 起点不越过上一 run 终点（避免重复画同一字符）。
    let mut prev_local_end: Option<usize> = None;

    for run in runs {
        let run_start = cursor;
        let run_end = cursor + run.len;

        if run_end <= range.start {
            cursor = run_end;
            continue;
        }

        if run_start >= range.end {
            break;
        }

        // 在 line 内的绝对字节偏移（line 起点 = line_offset）
        let line_local_start = range.start - line_offset;
        let line_local_end = range.end - line_offset;
        let clipped_start = range.start.max(run_start) - line_offset;
        let clipped_end = range.end.min(run_end) - line_offset;

        // start 用 floor（防与上一 run 重叠），end 用 ceil（不吞完整字符）。
        // 下一 run 起点必须 ≥ 上一 run 在 line 内的终点。
        let floor_local_start = floor_char_boundary(line, clipped_start.min(line.len()));
        let mut safe_start = floor_local_start.max(line_local_start);
        if let Some(prev_end) = prev_local_end {
            safe_start = safe_start.max(prev_end);
        }
        let safe_end_raw = ceil_char_boundary(line, clipped_end.min(line.len()));
        let safe_end = safe_end_raw.max(safe_start).min(line_local_end);
        let len = safe_end - safe_start;

        if len > 0 {
            result.push(TextRun { len, ..run.clone() });
            prev_local_end = Some(safe_end);
        }

        cursor = run_end;
    }

    result
}

/// 终末防御：对每个 `TextRun.len` 累加向前 **ceil** clip 到 `sub_line` 的字符边界。
/// GPUI 的 `shape_line` 内部走 `text.split_at(run.len)`，要求 `run.len` 必须是
/// `sub_line` 的字符边界。前置 layer（wrap range / runs_for_range /
/// split_runs_by_bg_segments）已分别 clip，但仍可能有遗漏的 byte↔char 错位
/// （IME marked range、CJK wrap、tree-sitter highlight 边界等）。
///
/// ceil 而非 floor：让每个 run 延伸到字符尾，避免吞完整字符；
/// 下一 run 起点（由 ceil 的累计值）仍是字符边界，char-safe。
fn char_safe_runs(sub_line: &str, mut runs: Vec<TextRun>) -> Vec<TextRun> {
    let mut acc = 0usize;
    for r in &mut runs {
        let new_acc = ceil_char_boundary(sub_line, acc + r.len);
        r.len = new_acc.saturating_sub(acc);
        acc = new_acc;
    }
    runs
}

fn split_runs_by_bg_segments(
    line: &str,
    start_offset: usize,
    runs: &[TextRun],
    bg_segments: &[(Range<usize>, Hsla)],
) -> Vec<TextRun> {
    let mut result = vec![];

    let mut cursor = start_offset;
    for run in runs {
        let mut run_start = cursor;
        let run_end = cursor + run.len;

        // 把 run 内的 [run_start, run_end] 钳到 line 的字符边界：
        // start 用 floor（与上一 run 接续），end 用 ceil（不吞完整 CJK 字符）
        let run_local_start = floor_char_boundary(line, (run_start - start_offset).min(line.len()));
        let run_local_end =
            ceil_char_boundary(line, (run_end - start_offset).min(line.len())).max(run_local_start);
        run_start = start_offset + run_local_start;
        let run_end_safe = start_offset + run_local_end;

        for (raw_bg_range, bg_color) in bg_segments {
            // bg_range 是全文绝对字节偏移。钳到 line 字符边界：
            // start 用 floor、end 用 ceil，避免把选区终点落在字符中间导致
            // 该字符不被染色。
            let bg_range = {
                let local_start = (raw_bg_range.start.saturating_sub(start_offset)).min(line.len());
                let local_end = (raw_bg_range.end.saturating_sub(start_offset)).min(line.len());
                if local_end == 0 || local_start >= line.len() {
                    continue;
                }
                let safe_local_start = floor_char_boundary(line, local_start);
                let safe_local_end = ceil_char_boundary(line, local_end).max(safe_local_start);
                if safe_local_start >= safe_local_end {
                    continue;
                }
                let safe_start = start_offset + safe_local_start;
                let safe_end = start_offset + safe_local_end;
                safe_start..safe_end
            };

            if run_end_safe <= bg_range.start || run_start >= bg_range.end {
                continue;
            }

            // Overlap exists
            if run_start < bg_range.start {
                // Add the part before the background range
                result.push(TextRun {
                    len: bg_range.start - run_start,
                    ..run.clone()
                });
            }

            // Add the overlapping part with background color
            let overlap_start = run_start.max(bg_range.start);
            let overlap_end = run_end_safe.min(bg_range.end);
            let text_color = if bg_color.l >= 0.5 {
                gpui::black()
            } else {
                gpui::white()
            };

            let run_len = overlap_end.saturating_sub(overlap_start);
            if run_len > 0 {
                result.push(TextRun {
                    len: run_len,
                    color: text_color,
                    ..run.clone()
                });

                cursor = bg_range.end;
                run_start = cursor;
            }
        }

        if run_end_safe > cursor {
            // Add the part after the background range
            result.push(TextRun {
                len: run_end_safe - cursor,
                ..run.clone()
            });
        }

        cursor = run_end;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, Context, ParentElement, Render, Styled, div};

    struct CjkWrapView {
        input: Entity<InputState>,
    }

    impl Render for CjkWrapView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            // 窄宽度强制软换行
            div()
                .w(px(100.))
                .child(crate::input::Input::new(&self.input))
        }
    }

    /// 端到端复现：多行中文 + 软换行 + 滚动视口内反复 Shift+Up/Down 扩选，
    /// 不得触发 UTF-8 字符边界 panic（split_at / slice）。
    #[gpui::test]
    fn shift_select_vertical_with_wrapped_cjk_does_not_panic(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| crate::init(cx));

        let long_line = "前5的进程/应用的简介（包括但".repeat(20);
        let text = format!("{0}\n{0}\n{0}\n{0}", long_line);

        let input_stash = Rc::new(std::cell::RefCell::new(None::<Entity<InputState>>));
        let stash = input_stash.clone();
        let (_root, visual_cx) = cx.add_window_view(|window, cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .auto_grow(1, 2)
                    .default_value(text.clone())
            });
            *stash.borrow_mut() = Some(input.clone());
            let view = cx.new(|_cx| CjkWrapView { input });
            crate::Root::new(view, window, cx)
        });
        let input = input_stash.borrow().clone().unwrap();

        // 聚焦并把光标移到文末
        visual_cx.update(|window, cx| {
            input.update(cx, |state, cx| {
                state.focus(window, cx);
                let end = state.text.len();
                state.move_to(end, None, cx);
            });
        });

        use crate::actions::{SelectDown, SelectUp};
        for _ in 0..12 {
            visual_cx.update(|window, cx| {
                input.update(cx, |state, cx| state.select_up(&SelectUp, window, cx));
            });
        }
        for _ in 0..24 {
            visual_cx.update(|window, cx| {
                input.update(cx, |state, cx| state.select_down(&SelectDown, window, cx));
            });
        }
        for _ in 0..12 {
            visual_cx.update(|window, cx| {
                input.update(cx, |state, cx| state.select_up(&SelectUp, window, cx));
            });
        }
    }

    /// 回归测试：多行输入滚动后（visible_range.start > 0），纯文本/IME 分支的 runs
    /// 按全文绝对坐标构建，必须先在 prepaint 重定基到可见范围，再交给 layout_lines
    /// 用局部偏移切分；否则 run 边界错位落在 UTF-8 字符中间，
    /// MacTextSystem::layout_line 的 split_at 会 panic：
    /// `byte index 12 is not a char boundary; it is inside '程' (bytes 10..13)`。
    #[test]
    fn scrolled_input_runs_rebase_keeps_char_boundaries() {
        let visible_line = "前5的进程/应用的简介（包括但";
        let first_line = "占位第一行文本";
        let full_text = format!("{first_line}\n{visible_line}");
        // 第二行的绝对字节范围（即 visible_range.start = 1 时）
        let visible_start = first_line.len() + 1;
        let visible_end = full_text.len();

        let base = TextRun {
            len: 0,
            font: gpui::font(".SystemUIFont"),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // 绝对坐标 runs：IME marked range 在绝对偏移 12 处切了一刀
        let runs = vec![
            TextRun {
                len: 12,
                ..base.clone()
            },
            TextRun {
                len: full_text.len() - 12,
                ..base.clone()
            },
        ];

        // bug 版本流程：直接拿绝对坐标 runs 用局部偏移手工切分（绕过 runs_for_range
        // 的 char-boundary 校准），模拟修复前行为：第一刀 len=12 落在 '程'(bytes 10..13) 中间
        let mut acc = 0;
        let buggy_boundary_misaligned = runs.iter().any(|run| {
            acc += run.len;
            !visible_line.is_char_boundary(acc)
        });
        assert!(buggy_boundary_misaligned);

        // 修复后流程：prepaint 先把 runs 裁剪重定基到可见范围，再交给 runs_for_range
        // 用字符边界校准切出可见行的 runs（每个 run.len 必须落在 char boundary 上）
        let rebased = runs_for_range(visible_line, &runs, 0, &(visible_start..visible_end));
        let line_runs = runs_for_range(visible_line, &rebased, 0, &(0..visible_line.len()));
        let mut acc = 0;
        let mut last_boundary = 0usize;
        for run in &line_runs {
            acc += run.len;
            assert!(
                visible_line.is_char_boundary(acc),
                "run boundary {acc} is not a char boundary of {visible_line:?}"
            );
            last_boundary = acc;
        }
        // 校准到字符边界后，acc 只能小于等于 line 长度，且最后一个 run 必须把可见行
        // 推到 line 末尾——否则 GPUI 在 shape_line 时按 run.len 切 sub_line 会剩尾巴未渲染
        assert!(
            last_boundary <= visible_line.len(),
            "run boundary 越过 line 末尾: {last_boundary} > {}",
            visible_line.len()
        );
        // 把 line_runs 拼起来长度必须等于 last_boundary（说明累加无误）
        let total: usize = line_runs.iter().map(|r| r.len).sum();
        assert_eq!(total, last_boundary, "runs 累加自身必须一致");
    }

    #[test]
    fn test_runs_for_range() {
        let run = TextRun {
            len: 0,
            font: gpui::font(".SystemUIFont"),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        // use hello this-is-test
        let runs = vec![
            // use
            TextRun {
                len: 3,
                ..run.clone()
            },
            // \s
            TextRun {
                len: 1,
                ..run.clone()
            },
            // hello
            TextRun {
                len: 5,
                ..run.clone()
            },
            // \s
            TextRun {
                len: 1,
                ..run.clone()
            },
            // this-is-test
            TextRun {
                len: 12,
                ..run.clone()
            },
        ];

        #[track_caller]
        fn assert_runs(actual: Vec<TextRun>, expected: &[usize]) {
            let left = actual.iter().map(|run| run.len).collect::<Vec<_>>();
            assert_eq!(left, expected);
        }

        let ascii_line = "use hello this-is-test";
        assert_runs(runs_for_range(ascii_line, &runs, 0, &(0..0)), &[]);
        assert_runs(
            runs_for_range(ascii_line, &runs, 0, &(0..100)),
            &[3, 1, 5, 1, 12],
        );

        assert_runs(runs_for_range(ascii_line, &runs, 0, &(0..6)), &[3, 1, 2]);
        assert_runs(runs_for_range(ascii_line, &runs, 0, &(1..6)), &[2, 1, 2]);
        assert_runs(runs_for_range(ascii_line, &runs, 0, &(3..10)), &[1, 5, 1]);
        assert_runs(runs_for_range(ascii_line, &runs, 0, &(5..8)), &[3]);
        assert_runs(runs_for_range(ascii_line, &runs, 3, &(0..3)), &[1, 2]);
        assert_runs(runs_for_range(ascii_line, &runs, 3, &(2..10)), &[4, 1, 3]);
        assert_runs(runs_for_range(ascii_line, &runs, 9, &(0..8)), &[1, 7]);
    }

    #[test]
    fn runs_for_range_clamps_byte_offsets_to_char_boundaries_for_cjk() {
        // 模拟真实场景：tree-sitter highlight_styles 把 IME marked range
        // 切在字符中间（byte offset 12 落在 '程' (10..13) 内）。GPUI 的
        // MacTextSystem::layout_line 内部 text.split_at(run.len) 在 run.len
        // 不是字符边界时 panic。修复后 runs_for_range 必须把每个 run.len
        // floor 到 line 的字符边界，杜绝 panic。
        let visible_line = "前5的进程/应用的简介（包括但"; // 41 bytes
        // IME 在绝对 offset 12 处切了一刀（line_offset=0）
        let base = TextRun {
            len: 0,
            font: gpui::font(".SystemUIFont"),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = vec![
            TextRun {
                len: 12, // 12 落在 '程' (10..13) 中间
                ..base.clone()
            },
            TextRun {
                len: visible_line.len() - 12,
                ..base.clone()
            },
        ];

        let line_runs = runs_for_range(visible_line, &runs, 0, &(0..visible_line.len()));
        // 每个累加 acc 必须是 char boundary（这是 shape_line 不 panic 的充要条件）
        let mut acc = 0usize;
        for run in &line_runs {
            acc += run.len;
            assert!(
                visible_line.is_char_boundary(acc),
                "run boundary {acc} is not a char boundary of {visible_line:?}"
            );
        }
        // 累加总和必须等于 line 长度（"不丢字"不变量）
        assert_eq!(
            acc,
            visible_line.len(),
            "runs 累加必须等于 line.len()（不丢字）"
        );
        // 每个 run.len 必须落在 [1, visible_line.len()] 之间（不能 0 也不能越界）
        for run in &line_runs {
            assert!(run.len > 0, "run.len must be > 0");
            assert!(
                run.len <= visible_line.len(),
                "run.len={} > line len",
                run.len
            );
        }
    }

    #[test]
    fn split_runs_by_bg_segments_clamps_to_char_boundaries_for_cjk() {
        // 用户实测 panic 路径：选区字节偏移跨字符中间。
        // selection_bg_segments 给出 (raw_start..raw_end, color) 落在 CJK 字符内
        // （如 '应' bytes 0..3 内），split_runs_by_bg_segments 必须把 bg_range
        // 的 start/end 都钳到 line 的字符边界，下游 shape_line 才不 panic。
        let run = TextRun {
            len: 0,
            font: gpui::font(".SystemUIFont"),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        // line 字符边界：0..2="CPU", 2..8="占用前", 8..10="5的", 10..16="进程应"
        let line = "CPU占用前5的进程应用";
        // 假设选区起点 9 落在 '的' (8..10) 中间；终点 16 是 '用' 后边界
        // raw bg_range: [9..16]，期望被钳到 [10..16]（'进' 起点）
        let bg_segments = vec![(9..16usize, gpui::red())];
        let runs = vec![TextRun {
            len: line.len(),
            ..run.clone()
        }];

        let result = split_runs_by_bg_segments(line, 0, &runs, &bg_segments);
        // 累加每个 TextRun.len 必须落在 line 的字符边界上
        let mut acc = 0usize;
        for r in &result {
            acc += r.len;
            assert!(
                line.is_char_boundary(acc),
                "bg-clipped run boundary {acc} not a char boundary of {line:?}"
            );
        }
        // bg_range 起点 floor 到 10（'进' 起点），所以染色段 len = 16 - 10 = 6，
        // 染色段前面非染色段 len = 10 - 0 = 10；累加 10 + 6 = 16 == line.len()。
        assert_eq!(acc, line.len(), "累加必须等于 line.len()（不丢字）");
        assert!(
            result.len() >= 2,
            "应有 ≥2 个 run：未染色 + 染色，实际 {}",
            result.len()
        );
    }

    #[test]
    fn char_safe_runs_clamps_to_char_boundaries() {
        // 终末防御：累加 byte 落在字符中间时，向前 floor clip。
        // '程' bytes 22..25：累加 24 落在中间 → floor(24)=22 → run.len=3
        let sub_line = "前5的进程/应用的简介";
        let base = TextRun {
            len: 0,
            font: gpui::font(".SystemUIFont"),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // 故意造一个会触发 panic 的输入：
        // run0.len=12 + run1.len=29 = 41，但 run0 累加 12 落在 '程' (10..13) 中间
        let runs = vec![
            TextRun {
                len: 12,
                ..base.clone()
            },
            TextRun {
                len: 29,
                ..base.clone()
            },
        ];
        let safe = char_safe_runs(sub_line, runs);
        // 每个累加 byte 必须落在 sub_line 的字符边界上
        let mut acc = 0usize;
        for r in &safe {
            acc += r.len;
            assert!(
                sub_line.is_char_boundary(acc),
                "run boundary {acc} not a char boundary of {sub_line:?}"
            );
        }
        // 最终 acc 必须 ≤ sub_line.len()（多余字符归到末尾 run）
        assert!(acc <= sub_line.len());
    }

    #[test]
    fn test_split_runs_by_bg_segments() {
        let run = TextRun {
            len: 0,
            font: gpui::font(".SystemUIFont"),
            color: gpui::blue(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = vec![
            TextRun {
                len: 5,
                ..run.clone()
            },
            TextRun {
                len: 7,
                ..run.clone()
            },
            TextRun {
                len: 24,
                ..run.clone()
            },
        ];

        let bg_segments = vec![(8..12, gpui::red()), (12..18, gpui::blue())];
        // ASCII 测试串：line 长度 36，floor clip 是 no-op，结果与之前相同
        let ascii_line = "abcdefghijklmnopqrstuvwxyz0123456789ab";
        let result = split_runs_by_bg_segments(ascii_line, 5, &runs, &bg_segments);
        assert_eq!(
            result.iter().map(|run| run.len).collect::<Vec<_>>(),
            vec![3, 2, 2, 5, 1, 23]
        );
        assert_eq!(result[0].color, gpui::blue());
        assert_eq!(result[1].color, gpui::black());
        assert_eq!(result[2].color, gpui::black());
        assert_eq!(result[3].color, gpui::black());
        assert_eq!(result[4].color, gpui::black());
        assert_eq!(result[5].color, gpui::blue());
    }
}

/// 把 byte offset 向下对齐到最近的字符边界（panic-safe 取 range.start）。
/// 如果 offset 已在字符边界，原样返回；否则回退到前一个边界。
/// 用于防御 GPUI LineWrapper 按 byte 算 wrap 范围时落在 CJK 等多字节字符中间。
fn floor_char_boundary(s: &str, offset: usize) -> usize {
    if offset >= s.len() {
        return s.len();
    }
    if s.is_char_boundary(offset) {
        return offset;
    }
    // 二分查找前一个字符边界
    let mut lo = 0usize;
    let mut hi = offset;
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if s.is_char_boundary(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// 把 byte offset 向上对齐到最近的字符边界（panic-safe 取 range.end）。
/// 如果 offset 已在字符边界，原样返回；否则前进到下一个边界。
fn ceil_char_boundary(s: &str, offset: usize) -> usize {
    if offset >= s.len() {
        return s.len();
    }
    if s.is_char_boundary(offset) {
        return offset;
    }
    // 二分查找后一个字符边界
    let mut lo = offset;
    let mut hi = s.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if s.is_char_boundary(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

#[cfg(test)]
mod char_boundary_tests {
    use super::{ceil_char_boundary, floor_char_boundary};

    #[test]
    fn floor_aligns_to_previous_char_boundary() {
        // '名' = UTF-8 bytes 22..25
        // 0..24 中含 byte 24（落在 '名' 中间）
        let s = "0123456789012345678901名rest";
        // offset=24 应回退到 22（'名' 起点）
        assert_eq!(floor_char_boundary(s, 24), 22);
        // 已在边界
        assert_eq!(floor_char_boundary(s, 22), 22);
        // 超过长度
        assert_eq!(floor_char_boundary(s, 1000), s.len());
    }

    #[test]
    fn ceil_aligns_to_next_char_boundary() {
        let s = "0123456789012345678901名rest";
        // offset=24 应前进到 25（'名' 终点）
        assert_eq!(ceil_char_boundary(s, 24), 25);
        // 已在边界
        assert_eq!(ceil_char_boundary(s, 22), 22);
        assert_eq!(ceil_char_boundary(s, 25), 25);
        // 超过长度
        assert_eq!(ceil_char_boundary(s, 1000), s.len());
    }

    #[test]
    fn handles_pure_ascii() {
        let s = "hello world";
        // 全部是字符边界
        for i in 0..=s.len() {
            assert_eq!(floor_char_boundary(s, i), i);
            assert_eq!(ceil_char_boundary(s, i), i);
        }
    }

    #[test]
    fn floor_handles_empty_and_overflow_and_emoji() {
        // 空串
        assert_eq!(floor_char_boundary("", 0), 0);
        // offset > len → clamp
        assert_eq!(floor_char_boundary("abc", 10), 3);
        // 4 字节 emoji（U+1F600 😀 bytes 0..4）
        let s = "😀rest";
        assert_eq!(floor_char_boundary(s, 0), 0);
        assert_eq!(floor_char_boundary(s, 1), 0);
        assert_eq!(floor_char_boundary(s, 2), 0);
        assert_eq!(floor_char_boundary(s, 3), 0);
        assert_eq!(floor_char_boundary(s, 4), 4);
    }

    #[test]
    fn same_direction_floor_clip_never_duplicates_or_drops_cjk() {
        // s 字符边界（实测）：
        //   0..22  = "0123456789012345678901"
        //   22..25 = "名"
        //   25..29 = "rest"
        //   29..32 = "世"  ← 30 是 '世' 中间
        //   32..35 = "界"
        //   35..38 = "abc"
        let s = "0123456789012345678901名rest世界abc";

        // 模拟 GPUI LineWrapper 给出的相邻 wrap range，共享边界落在字符中间：
        //   r0=[0..24]  尾 24 落在 '名' 内 → floor=22（只剩 22 个 ASCII）
        //   r1=[24..30] 头 24 落在 '名' 内 → floor=22; 尾 30 落在 '世' 内 → floor=29
        //   r2=[30..35] 头 30 落在 '世' 内 → floor=29; 尾 35 是 '界' 后边界
        let raw_ranges = [0..24usize, 24..30, 30..35];
        let clipped: Vec<std::ops::Range<usize>> = raw_ranges
            .iter()
            .map(|r| {
                let start = floor_char_boundary(s, r.start.min(s.len()));
                let end = floor_char_boundary(s, r.end.min(s.len())).max(start);
                start..end
            })
            .filter(|r| r.end > r.start)
            .collect();

        // 拼接后必须等于 s[0..35] —— 既不丢字也不重字
        let joined: String = clipped.iter().map(|r| &s[r.clone()]).collect();
        assert_eq!(joined, &s[0..35]);
        // 关键不变量：clipped 内的 range 之间首尾相接，没有重叠
        for w in clipped.windows(2) {
            assert_eq!(w[0].end, w[1].start, "adjacent slices must be contiguous");
        }
    }
}
