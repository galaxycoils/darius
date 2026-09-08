//! Minimal CSI screen reconstruction for PTY assertions (not a production emulator).
pub fn render(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut chars = text.chars().peekable();
    let mut grid = vec![vec![' '; 120]; 40];
    let (mut row, mut col) = (0usize, 0usize);
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() != Some('[') {
                continue;
            }
            let mut params = String::new();
            let mut op = 'm';
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    op = c;
                    break;
                }
                params.push(c);
            }
            let nums: Vec<usize> = params.split(';').map(|v| v.parse().unwrap_or(0)).collect();
            let n = nums.first().copied().unwrap_or(0);
            match op {
                'H' | 'f' => {
                    row = n.max(1) - 1;
                    col = nums.get(1).copied().unwrap_or(1).max(1) - 1;
                }
                'G' => col = n.max(1) - 1,
                'd' => row = n.max(1) - 1,
                'A' => row = row.saturating_sub(n.max(1)),
                'B' => row += n.max(1),
                'C' => col += n.max(1),
                'D' => col = col.saturating_sub(n.max(1)),
                'J' if n == 2 => grid.iter_mut().for_each(|r| r.fill(' ')),
                'K' if row < 40 => match n {
                    2 => grid[row].fill(' '),
                    0 => grid[row][col.min(120)..].fill(' '),
                    _ => {}
                },
                _ => {}
            }
        } else {
            match c {
                '\r' => col = 0,
                '\n' => row += 1,
                c if !c.is_control() => {
                    if row < 40 && col < 120 {
                        grid[row][col] = c;
                    }
                    col += 1;
                }
                _ => {}
            }
        }
    }
    grid.into_iter()
        .map(|r| r.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
