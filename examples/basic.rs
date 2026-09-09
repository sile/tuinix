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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal driver and query its size
    let mut driver = tuinix::TerminalDriver::new()?;
    let mut state = tuinix::TerminalState::new(driver.size()?);

    // Create a frame with the terminal's dimensions
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());

    // Add styled content to the frame
    let title_style = tuinix::TerminalStyle::new()
        .bold()
        .fg_color(tuinix::TerminalColor::GREEN);

    write_text(&mut frame, "Welcome to tuinix!\n", title_style);
    write_text(
        &mut frame,
        "\nPress any key ('q' to quit)\n",
        tuinix::TerminalStyle::new(),
    );

    // Render the frame to a byte buffer, then write it to the terminal.
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
            let tuinix::TerminalInput::Key(input) = input else {
                continue; // Skip mouse events
            };

            // Check if 'q' was pressed
            if let tuinix::KeyCode::Char('q') = input.code {
                break;
            }

            // Display the input
            let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());
            write_text(
                &mut frame,
                &format!("Key pressed: {:?}\n", input),
                tuinix::TerminalStyle::new(),
            );
            write_text(
                &mut frame,
                "\nPress any key ('q' to quit)\n",
                tuinix::TerminalStyle::new(),
            );
            let mut out = Vec::new();
            state.render(frame, &mut out);
            driver.write_all(&out)?;
            driver.flush()?;
        }

        // Poll for read readiness on the input file descriptor. Here we just
        // read as much as is available without blocking on a selector.
        let n = driver.read(&mut raw)?;
        if n == 0 {
            continue;
        }
        state.push_input(&raw[..n]);
    }

    Ok(())
}
