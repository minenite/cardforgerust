//! Legacy chat formatting (`&` / `§`), Bukkit-style.

/// Translate `&` alternate color codes to Minecraft section (`§`) codes.
///
/// Same idea as Bukkit `ChatColor.translateAlternateColorCodes('&', …)`:
/// `&aHello &7world` → `§aHello §7world`. A doubled `&&` becomes a literal `&`.
pub fn colorize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('&') => {
                chars.next();
                out.push('&');
            }
            Some(code) if is_format_code(code) => {
                chars.next();
                out.push('§');
                out.push(code);
            }
            _ => out.push('&'),
        }
    }
    out
}

fn is_format_code(c: char) -> bool {
    matches!(
        c.to_ascii_lowercase(),
        '0'..='9' | 'a'..='f' | 'k'..='o' | 'r' | 'x'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ampersand_to_section() {
        assert_eq!(colorize("&aHello &7world"), "§aHello §7world");
        assert_eq!(colorize("&&a"), "&a");
        assert_eq!(colorize("plain"), "plain");
    }
}
