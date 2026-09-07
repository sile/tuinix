use std::time::Duration;

fn write_text(frame: &mut tuinix::TerminalFrame, text: &str, style: tuinix::TerminalStyle) {
    for c in text.chars() {
        match c {
            '\n' => frame.push_newline(),
            '\t' => frame.push_tab(8),
            c if c.is_control() => {}
            c => frame.push_char(c, 1, style),
        }
    }
}

fn draw_header(
    frame: &mut tuinix::TerminalFrame,
    title_style: tuinix::TerminalStyle,
    info_style: tuinix::TerminalStyle,
) {
    write_text(frame, "Mouse Input Demo\n", title_style);
    write_text(frame, "\nInstructions:\n", info_style);
    write_text(
        frame,
        "• Click anywhere to see mouse events\n",
        tuinix::TerminalStyle::new(),
    );
    write_text(
        frame,
        "• Try left, right, and middle mouse buttons\n",
        tuinix::TerminalStyle::new(),
    );
    write_text(
        frame,
        "• Try scrolling with the mouse wheel\n",
        tuinix::TerminalStyle::new(),
    );
    write_text(frame, "• Press 'q' to quit\n", tuinix::TerminalStyle::new());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal
    let mut terminal = tuinix::Terminal::new()?;

    // Enable mouse input reporting
    terminal.enable_mouse_input()?;

    // Create a frame with the terminal's dimensions
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(terminal.size());

    // Add styled content to the frame
    let title_style = tuinix::TerminalStyle::new().bold();
    let info_style = tuinix::TerminalStyle::new().underline();

    draw_header(&mut frame, title_style, info_style);
    write_text(&mut frame, "\nLast mouse event: None\n", info_style);

    // Draw the initial frame to the terminal
    terminal.draw(frame)?;

    // Process input events with a timeout
    loop {
        match terminal.poll_event(&[], &[], Some(Duration::from_millis(100)))? {
            Some(tuinix::TerminalEvent::Input(input)) => {
                match input {
                    tuinix::TerminalInput::Key(key_input) => {
                        // Check if 'q' was pressed
                        if let tuinix::KeyCode::Char('q') = key_input.code {
                            break;
                        }

                        // Display the key input
                        let mut frame: tuinix::TerminalFrame =
                            tuinix::TerminalFrame::new(terminal.size());
                        draw_header(&mut frame, title_style, info_style);
                        write_text(
                            &mut frame,
                            &format!("\nLast event: Key pressed: {:?}\n", key_input),
                            info_style,
                        );
                        terminal.draw(frame)?;
                    }
                    tuinix::TerminalInput::Mouse(mouse_input) => {
                        // Display the mouse input with detailed information
                        let mut frame: tuinix::TerminalFrame =
                            tuinix::TerminalFrame::new(terminal.size());
                        draw_header(&mut frame, title_style, info_style);

                        // Format mouse event details
                        let event_style = tuinix::TerminalStyle::new()
                            .bold()
                            .fg_color(tuinix::TerminalColor::GREEN);
                        write_text(&mut frame, "\nMouse Event Details:\n", event_style);
                        write_text(
                            &mut frame,
                            &format!("  Event: {:?}\n", mouse_input.event),
                            tuinix::TerminalStyle::new(),
                        );
                        write_text(
                            &mut frame,
                            &format!(
                                "  Position: column {}, row {}\n",
                                mouse_input.position.col, mouse_input.position.row
                            ),
                            tuinix::TerminalStyle::new(),
                        );

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
                            write_text(
                                &mut frame,
                                &format!("  Modifiers: {}\n", modifiers.join(" + ")),
                                tuinix::TerminalStyle::new(),
                            );
                        } else {
                            write_text(
                                &mut frame,
                                "  Modifiers: None\n",
                                tuinix::TerminalStyle::new(),
                            );
                        }

                        // Add event-specific information
                        match mouse_input.event {
                            tuinix::MouseEvent::LeftPress => write_text(
                                &mut frame,
                                "  → Left button pressed\n",
                                tuinix::TerminalStyle::new(),
                            ),
                            tuinix::MouseEvent::LeftRelease => write_text(
                                &mut frame,
                                "  → Left button released\n",
                                tuinix::TerminalStyle::new(),
                            ),
                            tuinix::MouseEvent::RightPress => write_text(
                                &mut frame,
                                "  → Right button pressed\n",
                                tuinix::TerminalStyle::new(),
                            ),
                            tuinix::MouseEvent::RightRelease => write_text(
                                &mut frame,
                                "  → Right button released\n",
                                tuinix::TerminalStyle::new(),
                            ),
                            tuinix::MouseEvent::MiddlePress => write_text(
                                &mut frame,
                                "  → Middle button pressed\n",
                                tuinix::TerminalStyle::new(),
                            ),
                            tuinix::MouseEvent::MiddleRelease => write_text(
                                &mut frame,
                                "  → Middle button released\n",
                                tuinix::TerminalStyle::new(),
                            ),
                            tuinix::MouseEvent::Drag => write_text(
                                &mut frame,
                                "  → Mouse dragged\n",
                                tuinix::TerminalStyle::new(),
                            ),
                            tuinix::MouseEvent::ScrollUp => write_text(
                                &mut frame,
                                "  → Scrolled up\n",
                                tuinix::TerminalStyle::new(),
                            ),
                            tuinix::MouseEvent::ScrollDown => write_text(
                                &mut frame,
                                "  → Scrolled down\n",
                                tuinix::TerminalStyle::new(),
                            ),
                        }

                        terminal.draw(frame)?;
                    }
                }
            }
            Some(tuinix::TerminalEvent::Resize(size)) => {
                // Terminal was resized, update UI
                let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
                draw_header(&mut frame, title_style, info_style);
                write_text(
                    &mut frame,
                    &format!("\nTerminal resized to {}x{}\n", size.cols, size.rows),
                    info_style,
                );
                terminal.draw(frame)?;
            }
            Some(tuinix::TerminalEvent::FdReady { .. }) => unreachable!(),
            None => {
                // Timeout elapsed, no events to process
            }
        }
    }

    Ok(())
}
