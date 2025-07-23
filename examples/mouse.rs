use std::{fmt::Write, time::Duration};

use tuinix::{Terminal, TerminalColor, TerminalEvent, TerminalFrame, TerminalInput, TerminalStyle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal
    let mut terminal = Terminal::new()?;

    // Create a frame with the terminal's dimensions
    let mut frame: TerminalFrame = TerminalFrame::new(terminal.size());

    // Add styled content to the frame
    let title_style = TerminalStyle::new().bold();
    let info_style = TerminalStyle::new().underline();

    writeln!(
        frame,
        "{}Mouse Input Demo{}",
        title_style,
        TerminalStyle::RESET
    )?;
    writeln!(
        frame,
        "\n{}Instructions:{}",
        info_style,
        TerminalStyle::RESET
    )?;
    writeln!(frame, "• Click anywhere to see mouse events")?;
    writeln!(frame, "• Try left, right, and middle mouse buttons")?;
    writeln!(frame, "• Try scrolling with the mouse wheel")?;
    writeln!(frame, "• Press 'q' to quit")?;
    writeln!(
        frame,
        "\n{}Last mouse event: None{}",
        info_style,
        TerminalStyle::RESET
    )?;

    // Draw the initial frame to the terminal
    terminal.draw(frame)?;

    // Process input events with a timeout
    loop {
        match terminal.poll_event(&[], &[], Some(Duration::from_millis(100)))? {
            Some(TerminalEvent::Input(input)) => {
                match input {
                    TerminalInput::Key(key_input) => {
                        // Check if 'q' was pressed
                        if let tuinix::KeyCode::Char('q') = key_input.code {
                            break;
                        }

                        // Display the key input
                        let mut frame: TerminalFrame = TerminalFrame::new(terminal.size());
                        writeln!(
                            frame,
                            "{}Mouse Input Demo{}",
                            title_style,
                            TerminalStyle::RESET
                        )?;
                        writeln!(
                            frame,
                            "\n{}Instructions:{}",
                            info_style,
                            TerminalStyle::RESET
                        )?;
                        writeln!(frame, "• Click anywhere to see mouse events")?;
                        writeln!(frame, "• Try left, right, and middle mouse buttons")?;
                        writeln!(frame, "• Try scrolling with the mouse wheel")?;
                        writeln!(frame, "• Press 'q' to quit")?;
                        writeln!(
                            frame,
                            "\n{}Last event: Key pressed: {:?}{}",
                            info_style,
                            key_input,
                            TerminalStyle::RESET
                        )?;
                        terminal.draw(frame)?;
                    }
                    TerminalInput::Mouse(mouse_input) => {
                        // Display the mouse input with detailed information
                        let mut frame: TerminalFrame = TerminalFrame::new(terminal.size());
                        writeln!(
                            frame,
                            "{}Mouse Input Demo{}",
                            title_style,
                            TerminalStyle::RESET
                        )?;
                        writeln!(
                            frame,
                            "\n{}Instructions:{}",
                            info_style,
                            TerminalStyle::RESET
                        )?;
                        writeln!(frame, "• Click anywhere to see mouse events")?;
                        writeln!(frame, "• Try left, right, and middle mouse buttons")?;
                        writeln!(frame, "• Try scrolling with the mouse wheel")?;
                        writeln!(frame, "• Press 'q' to quit")?;

                        // Format mouse event details
                        let event_style =
                            TerminalStyle::new().bold().fg_color(TerminalColor::GREEN);
                        writeln!(
                            frame,
                            "\n{}Mouse Event Details:{}",
                            event_style,
                            TerminalStyle::RESET
                        )?;
                        writeln!(frame, "  Event: {:?}", mouse_input.event)?;
                        writeln!(
                            frame,
                            "  Position: column {}, row {}",
                            mouse_input.col, mouse_input.row
                        )?;

                        // Show modifiers if any are pressed
                        let mut modifiers = Vec::new();
                        if mouse_input.ctrl {
                            modifiers.push("Ctrl");
                        }
                        if mouse_input.alt {
                            modifiers.push("Alt");
                        }
                        if mouse_input.shift {
                            modifiers.push("Shift");
                        }

                        if !modifiers.is_empty() {
                            writeln!(frame, "  Modifiers: {}", modifiers.join(" + "))?;
                        } else {
                            writeln!(frame, "  Modifiers: None")?;
                        }

                        // Add event-specific information
                        match mouse_input.event {
                            tuinix::MouseEvent::LeftPress => {
                                writeln!(frame, "  → Left button pressed")?
                            }
                            tuinix::MouseEvent::LeftRelease => {
                                writeln!(frame, "  → Left button released")?
                            }
                            tuinix::MouseEvent::RightPress => {
                                writeln!(frame, "  → Right button pressed")?
                            }
                            tuinix::MouseEvent::RightRelease => {
                                writeln!(frame, "  → Right button released")?
                            }
                            tuinix::MouseEvent::MiddlePress => {
                                writeln!(frame, "  → Middle button pressed")?
                            }
                            tuinix::MouseEvent::MiddleRelease => {
                                writeln!(frame, "  → Middle button released")?
                            }
                            tuinix::MouseEvent::Drag => writeln!(frame, "  → Mouse dragged")?,
                            tuinix::MouseEvent::Move => writeln!(frame, "  → Mouse moved")?,
                            tuinix::MouseEvent::ScrollUp => writeln!(frame, "  → Scrolled up")?,
                            tuinix::MouseEvent::ScrollDown => writeln!(frame, "  → Scrolled down")?,
                        }

                        terminal.draw(frame)?;
                    }
                }
            }
            Some(TerminalEvent::Resize(size)) => {
                // Terminal was resized, update UI
                let mut frame: TerminalFrame = TerminalFrame::new(size);
                writeln!(
                    frame,
                    "{}Mouse Input Demo{}",
                    title_style,
                    TerminalStyle::RESET
                )?;
                writeln!(
                    frame,
                    "\n{}Instructions:{}",
                    info_style,
                    TerminalStyle::RESET
                )?;
                writeln!(frame, "• Click anywhere to see mouse events")?;
                writeln!(frame, "• Try left, right, and middle mouse buttons")?;
                writeln!(frame, "• Try scrolling with the mouse wheel")?;
                writeln!(frame, "• Press 'q' to quit")?;
                writeln!(
                    frame,
                    "\n{}Terminal resized to {}x{}{}",
                    info_style,
                    size.cols,
                    size.rows,
                    TerminalStyle::RESET
                )?;
                terminal.draw(frame)?;
            }
            Some(TerminalEvent::FdReady { .. }) => unreachable!(),
            None => {
                // Timeout elapsed, no events to process
            }
        }
    }

    Ok(())
}
