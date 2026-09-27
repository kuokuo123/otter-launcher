//! This module implements and describes common TTY methods & traits

/// Unsupported Terminals that don't support RAW mode
const UNSUPPORTED_TERM: [&str; 3] = ["dumb", "cons25", "emacs"];

use crate::config::Config;
use crate::highlight::Highlighter;
use crate::keys::KeyEvent;
use crate::layout::{GraphemeClusterMode, InputViewport, Layout, Position, Unit};
use crate::line_buffer::LineBuffer;
use crate::{Cmd, Prompt, Result};
use unicode_segmentation::UnicodeSegmentation as _;

/// Terminal state
pub trait RawMode: Sized {
    /// Disable RAW mode for the terminal.
    fn disable_raw_mode(&self) -> Result<()>;
}

/// Input event
pub enum Event {
    KeyPress(KeyEvent),
    ExternalPrint(String),
    #[cfg(target_os = "macos")]
    Timeout(bool),
}

/// Translate bytes read from stdin to keys.
pub trait RawReader {
    type Buffer;
    /// Blocking wait for either a key press or an external print
    fn wait_for_input(&mut self, single_esc_abort: bool) -> Result<Event>; // TODO replace calls to `next_key` by `wait_for_input` where relevant
    /// Blocking read of key pressed.
    fn next_key(&mut self, single_esc_abort: bool) -> Result<KeyEvent>;
    /// For CTRL-V support
    #[cfg(unix)]
    fn next_char(&mut self) -> Result<char>;
    /// Bracketed paste
    fn read_pasted_text(&mut self) -> Result<String>;
    /// Check if `key` is bound to a peculiar command
    fn find_binding(&self, key: &KeyEvent) -> Option<Cmd>;
    /// Backup type ahead
    fn unbuffer(self) -> Option<Buffer>;
}

/// Display prompt, line and cursor in terminal output
pub trait Renderer {
    type Reader: RawReader;

    fn move_cursor(&mut self, old: Position, new: Position) -> Result<()>;

    /// Display `prompt`, line and cursor in terminal output
    fn refresh_line<P: Prompt + ?Sized>(
        &mut self,
        prompt: &P,
        line: &LineBuffer,
        hint: Option<&str>,
        old_layout: Option<&Layout>, // used to clear old rows
        new_layout: &Layout,
        highlighter: Option<&dyn Highlighter>,
    ) -> Result<()>;

    /// Compute layout for rendering prompt + line + some info (either hint,
    /// validation msg, ...). on the screen. Depending on screen width, line
    /// wrapping may be applied.
    fn compute_layout(
        &self,
        prompt_size: Position,
        default_prompt: bool,
        line: &LineBuffer,
        info: Option<&str>,
        old_layout: Option<&Layout>,
    ) -> Layout {
        self.compute_layout_with_hint_wrap(
            prompt_size,
            default_prompt,
            line,
            info,
            old_layout,
            true,
        )
    }

    /// Compute a layout with the hint's actual terminal wrapping behavior.
    /// Prompt and input still wrap (or use the input viewport) as usual.
    fn compute_layout_with_hint_wrap(
        &self,
        prompt_size: Position,
        default_prompt: bool,
        line: &LineBuffer,
        info: Option<&str>,
        old_layout: Option<&Layout>,
        hint_wraps: bool,
    ) -> Layout {
        let pos = line.pos();
        // Leave the last column unused so writing a character cannot trigger
        // the terminal's automatic line wrap. When there is a hint, reserve
        // another cell for a one-line hint's placeholder. Explicit newlines
        // keep their usual layout, as do prompts without room for input.
        let right_edge = self
            .get_columns()
            .saturating_sub(self.horizontal_scroll_right_margin())
            .saturating_sub(if info.is_some() { 2 } else { 1 });
        let viewport = if self.horizontal_scroll()
            && !line.contains('\n')
            && prompt_size.col < right_edge
        {
            let mut start = old_layout
                .and_then(|layout| layout.input_viewport)
                .map_or(0, |view| view.start.min(pos));
            let moved_left_of_view = old_layout
                .and_then(|layout| layout.input_viewport)
                .is_some_and(|view| pos < view.start);
            // An edit can join two graphemes at the previous viewport edge.
            start = line[..pos]
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|&i| i <= start)
                .last()
                .unwrap_or(0);
            if moved_left_of_view {
                // Show the grapheme immediately before the cursor as well.
                start = line[..pos]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(i, _)| i);
            }

            let whole_line_end = self.calculate_position(line, prompt_size);
            if whole_line_end.row == prompt_size.row && whole_line_end.col <= right_edge {
                start = 0;
            } else {
                let current_cursor = self.calculate_position(&line[start..pos], prompt_size);
                if current_cursor.row != prompt_size.row || current_cursor.col > right_edge {
                    // Walk back from the cursor so a long paste does not
                    // require rescanning the whole line for each skipped cluster.
                    start = pos;
                    for (i, _) in line[..pos].grapheme_indices(true).rev() {
                        let candidate = self.calculate_position(&line[i..pos], prompt_size);
                        if candidate.row != prompt_size.row || candidate.col > right_edge {
                            break;
                        }
                        start = i;
                    }
                }
            }

            let mut end = start;
            let mut edge = prompt_size;
            for (i, grapheme) in line[start..].grapheme_indices(true) {
                let next = self.calculate_position(grapheme, edge);
                if next.row != prompt_size.row || next.col > right_edge {
                    break;
                }
                edge = next;
                end = start + i + grapheme.len();
            }
            Some(InputViewport { start, end })
        } else {
            None
        };

        let (cursor, mut end) = if let Some(view) = viewport {
            (
                self.calculate_position(&line[view.start..pos], prompt_size),
                self.calculate_position(&line[view.start..view.end], prompt_size),
            )
        } else {
            let cursor = self.calculate_position(&line[..pos], prompt_size);
            let end = if pos == line.len() {
                cursor
            } else {
                self.calculate_position(&line[pos..], cursor)
            };
            (cursor, end)
        };

        let mut new_layout = Layout {
            grapheme_cluster_mode: self.grapheme_cluster_mode(),
            prompt_size,
            default_prompt,
            cursor,
            end,
            has_info: info.is_some(),
            input_viewport: viewport,
        };
        if let Some(info) = new_layout.visible_hint(info, line.len()) {
            end = if hint_wraps {
                self.calculate_position(info, end)
            } else {
                self.calculate_position_no_wrap(info, end)
            };
            new_layout.end = end;
        }
        debug_assert!(new_layout.prompt_size <= new_layout.cursor);
        debug_assert!(new_layout.cursor <= new_layout.end);
        new_layout
    }

    /// Calculate the number of columns and rows used to display `s` on a
    /// `cols` width terminal starting at `orig`.
    fn calculate_position(&self, s: &str, orig: Position) -> Position;

    /// Position after text printed with automatic line wrapping disabled.
    /// Explicit newlines still move down; printable text stops at the edge.
    fn calculate_position_no_wrap(&self, s: &str, orig: Position) -> Position {
        self.calculate_position(s, orig)
    }

    fn write_and_flush(&mut self, buf: &str) -> Result<()>;

    /// Beep, used for completion when there is nothing to complete or when all
    /// the choices were already shown.
    fn beep(&mut self) -> Result<()>;

    /// Clear the screen. Used to handle ctrl+l
    fn clear_screen(&mut self) -> Result<()>;
    /// Clear rows used by prompt and edited line
    fn clear_rows(&mut self, layout: &Layout) -> Result<()>;
    /// Clear from cursor to the end of line
    fn clear_to_eol(&mut self) -> Result<()>;

    /// Update the number of columns/rows in the current terminal.
    fn update_size(&mut self);
    /// Get the number of columns in the current terminal.
    fn get_columns(&self) -> Unit;
    /// Whether the editor keeps the input on one visual row.
    fn horizontal_scroll(&self) -> bool {
        false
    }
    /// Columns reserved for neighboring content.
    fn horizontal_scroll_right_margin(&self) -> Unit {
        0
    }
    /// Get the number of rows in the current terminal.
    fn get_rows(&self) -> Unit;
    /// Check if output supports colors.
    fn colors_enabled(&self) -> bool;
    /// Tell how grapheme clusters are rendered.
    fn grapheme_cluster_mode(&self) -> GraphemeClusterMode;

    /// Make sure prompt is at the leftmost edge of the screen
    fn move_cursor_at_leftmost(&mut self, rdr: &mut Self::Reader) -> Result<()>;
    /// Begin synchronized update on unix platform
    fn begin_synchronized_update(&mut self) -> Result<()> {
        Ok(())
    }
    /// End synchronized update on unix platform
    fn end_synchronized_update(&mut self) -> Result<()> {
        Ok(())
    }
}

// ignore ANSI escape sequence
fn width(gcm: GraphemeClusterMode, s: &str, esc_seq: &mut u8) -> Unit {
    if *esc_seq == 1 {
        if s == "[" {
            // CSI
            *esc_seq = 2;
        } else {
            // two-character sequence
            *esc_seq = 0;
        }
        0
    } else if *esc_seq == 2 {
        if s == ";" || (s.as_bytes()[0] >= b'0' && s.as_bytes()[0] <= b'9') {
            /*} else if s == "m" {
            // last
             *esc_seq = 0;*/
        } else {
            // not supported
            *esc_seq = 0;
        }
        0
    } else if s == "\x1b" {
        *esc_seq = 1;
        0
    } else if s == "\n" {
        0
    } else {
        gcm.width(s)
    }
}

/// External printer
pub trait ExternalPrinter {
    /// Print message to stdout
    fn print(&mut self, msg: String) -> Result<()>;
}

/// Terminal contract
pub trait Term {
    type Buffer;
    type KeyMap;
    type Reader: RawReader<Buffer = Self::Buffer>; // rl_instream
    type Writer: Renderer<Reader = Self::Reader>; // rl_outstream
    type Mode: RawMode;
    type ExternalPrinter: ExternalPrinter;
    type CursorGuard;

    fn new(config: &Config) -> Result<Self>
    where
        Self: Sized;
    /// Check if current terminal can provide a rich line-editing user
    /// interface.
    fn is_unsupported(&self) -> bool;
    /// check if input stream is connected to a terminal.
    fn is_input_tty(&self) -> bool;
    /// check if output stream is connected to a terminal.
    fn is_output_tty(&self) -> bool;
    /// Enable RAW mode for the terminal.
    fn enable_raw_mode(&mut self, config: &Config) -> Result<(Self::Mode, Self::KeyMap)>;
    /// Create a RAW reader
    fn create_reader(
        &self,
        buffer: Option<Self::Buffer>,
        config: &Config,
        key_map: Self::KeyMap,
    ) -> Result<Self::Reader>;
    /// Create a writer
    fn create_writer(&self, config: &Config) -> Self::Writer;
    fn writeln(&self) -> Result<()>;
    /// Create an external printer
    fn create_external_printer(&mut self) -> Result<Self::ExternalPrinter>;
    /// Change cursor visibility
    fn set_cursor_visibility(&mut self, visible: bool) -> Result<Option<Self::CursorGuard>>;
}

/// Check TERM environment variable to see if current term is in our
/// unsupported list
fn is_unsupported_term() -> bool {
    match std::env::var("TERM") {
        Ok(term) => {
            for iter in &UNSUPPORTED_TERM {
                if (*iter).eq_ignore_ascii_case(&term) {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

// If on Windows platform import Windows TTY module
// and re-export into mod.rs scope
#[cfg(all(windows, not(target_arch = "wasm32")))]
mod windows;
#[cfg(all(windows, not(target_arch = "wasm32"), not(test)))]
pub use self::windows::*;

// If on Unix platform import Unix TTY module
// and re-export into mod.rs scope
#[cfg(all(unix, not(target_arch = "wasm32")))]
mod unix;
#[cfg(all(unix, not(target_arch = "wasm32"), not(test)))]
pub use self::unix::*;

#[cfg(any(test, target_arch = "wasm32"))]
mod test;
#[cfg(any(test, target_arch = "wasm32"))]
pub use self::test::*;

#[cfg(test)]
mod test_ {
    #[test]
    fn test_unsupported_term() {
        std::env::set_var("TERM", "xterm");
        assert!(!super::is_unsupported_term());

        std::env::set_var("TERM", "dumb");
        assert!(super::is_unsupported_term());
    }
}
