use ratatui::style::{Color, Style};
use ratatui::text::Span;

#[derive(Clone, Debug)]
pub enum WaitingFor {
    Provider,
    #[allow(clippy::upper_case_acronyms)]
    API,
}

#[derive(Clone, Debug)]
pub enum ConversationStatus {
    Waiting(WaitingFor),
    Passed,
    Failed,
    Warning,
}

pub const SPINNER_PHASE_SPREAD: usize = 4;
pub const DOTS_SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub const REVERSED_DOTS_SPINNER_FRAMES: &[char] =
    &['⠏', '⠇', '⠧', '⠦', '⠴', '⠼', '⠸', '⠹', '⠙', '⠋'];

pub fn get_status_indicator_spans(
    status: &ConversationStatus,
    conversation_id: i32,
    elapsed_spinner_frames: usize,
) -> Vec<Span<'_>> {
    let (status_char, status_color) = match status {
        ConversationStatus::Waiting(waiting_for) => {
            let phase_offset = usize::try_from(conversation_id).unwrap_or(0) * SPINNER_PHASE_SPREAD;
            match waiting_for {
                WaitingFor::Provider => {
                    let frame_idx = (elapsed_spinner_frames + phase_offset)
                        % REVERSED_DOTS_SPINNER_FRAMES.len();
                    (REVERSED_DOTS_SPINNER_FRAMES[frame_idx], Color::Magenta)
                }
                WaitingFor::API => {
                    let frame_idx =
                        (elapsed_spinner_frames + phase_offset) % DOTS_SPINNER_FRAMES.len();
                    (DOTS_SPINNER_FRAMES[frame_idx], Color::Blue)
                }
            }
        }
        ConversationStatus::Passed => ('✓', Color::Green),
        ConversationStatus::Failed => ('✗', Color::Red),
        ConversationStatus::Warning => ('▲', Color::Yellow),
    };
    vec![
        Span::raw("["),
        Span::styled(status_char.to_string(), Style::default().fg(status_color)),
        Span::raw("]"),
    ]
}
