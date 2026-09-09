use std::io::{Read, Write};

// NOTE: This is an ASCII-oriented demo helper: every character is assigned a width of 1.
// Non-ASCII characters (for example CJK or emoji) would need the caller to supply their
// actual width, because TerminalFrame does not compute character widths itself.
fn write_text(frame: &mut tuinix::TerminalFrame, text: &str, style: tuinix::TerminalStyle) {
    for c in text.chars() {
        match c {
            '\n' => frame.push_newline(),
            '\t' => frame.push_tab(8),
            c if c.is_control() => {}
            c => {
                frame.push_char(tuinix::TerminalChar::new(c, 1, style).expect("valid cell"));
            }
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
    // Initialize terminal driver and query its size
    let mut driver = tuinix::TerminalDriver::new()?;
    let mut state = tuinix::TerminalState::new(driver.size()?);

    // Enable mouse input reporting
    driver.enable_mouse_input()?;

    // Create a frame with the terminal's dimensions
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());

    // Add styled content to the frame
    let title_style = tuinix::TerminalStyle::new().bold();
    let info_style = tuinix::TerminalStyle::new().underline();

    draw_header(&mut frame, title_style, info_style);
    write_text(&mut frame, "\nLast mouse event: None\n", info_style);

    // Render the initial frame to the terminal.
    let mut out = Vec::new();
    state.render(frame, &mut out);
    driver.write_all(&out)?;
    driver.flush()?;

    // Process input events with a timeout
    let mut raw = [0u8; 256];
    loop {
        if state.has_pending_input()
            && let Some(input) = state.next_input()
        {
            match input {
                tuinix::TerminalInput::Key(key_input) => {
                    // Check if 'q' was pressed
                    if let tuinix::KeyCode::Char('q') = key_input.code {
                        break;
                    }

                    // Display the key input
                    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());
                    draw_header(&mut frame, title_style, info_style);
                    write_text(
                        &mut frame,
                        &format!("\nLast event: Key pressed: {:?}\n", key_input),
                        info_style,
                    );
                    let mut out = Vec::new();
                    state.render(frame, &mut out);
                    driver.write_all(&out)?;
                    driver.flush()?;
                }
                tuinix::TerminalInput::Mouse(mouse_input) => {
                    // Display the mouse input with detailed information
                    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());
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

                    let mut out = Vec::new();
                    state.render(frame, &mut out);
                    driver.write_all(&out)?;
                    driver.flush()?;
                }
            }
        }

        // Read raw bytes from the driver. In a real application this would be
        // driven by an event loop (see examples/nonblocking.rs); here we block
        // until input arrives.
        let n = driver.read(&mut raw)?;
        if n == 0 {
            continue;
        }
        state.feed_bytes(&raw[..n]);
    }

    Ok(())
}
