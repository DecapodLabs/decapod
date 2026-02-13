use std::env;

const MIN_BOX_WIDTH: usize = 40;
const MAX_BOX_WIDTH: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxStyle {
    Info,
    Success,
    Warning,
    Error,
    Cyan,
    Magenta,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemStatus {
    Created,
    Updated,
    Preserved,
    Unchanged,
    Skipped,
    Failed,
    Info,
    Pass,
    Fail,
}

impl ItemStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            ItemStatus::Created => "✨",
            ItemStatus::Updated => "🔄",
            ItemStatus::Preserved => "📎",
            ItemStatus::Unchanged => "➖",
            ItemStatus::Skipped => "⏭",
            ItemStatus::Failed => "💥",
            ItemStatus::Info => "💡",
            ItemStatus::Pass => "✅",
            ItemStatus::Fail => "❌",
        }
    }
}

pub fn terminal_width() -> usize {
    env::var("TERM_WIDTH")
        .ok()
        .and_then(|w| w.parse().ok())
        .or_else(|| env::var("COLUMNS").ok().and_then(|c| c.parse().ok()))
        .unwrap_or(80)
}

fn effective_width() -> usize {
    terminal_width().max(MIN_BOX_WIDTH).min(MAX_BOX_WIDTH)
}

fn indent() -> usize {
    (terminal_width().saturating_sub(effective_width())) / 2
}

pub fn box_top(width: usize) -> String {
    let w = width.max(MIN_BOX_WIDTH).min(effective_width());
    format!("╔{}{}╗", "═".repeat(w - 2), "═")
}

pub fn box_bottom(width: usize) -> String {
    let w = width.max(MIN_BOX_WIDTH).min(effective_width());
    format!("╚{}{}╝", "═".repeat(w - 2), "═")
}

pub fn box_row(left: &str, content: &str, right: &str, width: usize) -> String {
    let w = width.max(MIN_BOX_WIDTH).min(effective_width());
    let content_len = content.chars().count();
    let padding = w.saturating_sub(2).saturating_sub(content_len);
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;
    format!(
        "{}{}{}{}{}",
        left,
        " ".repeat(left_pad),
        content,
        " ".repeat(right_pad),
        right
    )
}

pub fn render_box(title: &str, subtitle: &str, style: BoxStyle) {
    use colored::Colorize;

    let width = effective_width();
    let indent_s = " ".repeat(indent());

    match style {
        BoxStyle::Info => {
            println!("{} 💙", indent_s);
            println!("{}{}", indent_s, box_top(width).bright_cyan());
            println!(
                "{}{}",
                indent_s,
                box_row("║", title, "║", width).bright_cyan().bold()
            );
            if !subtitle.is_empty() {
                println!("{}{}", indent_s, box_row("║", subtitle, "║", width).cyan());
            }
            println!("{}{}", indent_s, box_bottom(width).bright_cyan());
        }
        BoxStyle::Success => {
            println!("{} 💚", indent_s);
            println!("{}{}", indent_s, box_top(width).bright_green());
            println!(
                "{}{}",
                indent_s,
                box_row("║", title, "║", width).bright_green().bold()
            );
            if !subtitle.is_empty() {
                println!("{}{}", indent_s, box_row("║", subtitle, "║", width).green());
            }
            println!("{}{}", indent_s, box_bottom(width).bright_green());
        }
        BoxStyle::Warning => {
            println!("{} 💛", indent_s);
            println!("{}{}", indent_s, box_top(width).bright_yellow());
            println!(
                "{}{}",
                indent_s,
                box_row("║", title, "║", width).bright_yellow().bold()
            );
            if !subtitle.is_empty() {
                println!(
                    "{}{}",
                    indent_s,
                    box_row("║", subtitle, "║", width).yellow()
                );
            }
            println!("{}{}", indent_s, box_bottom(width).bright_yellow());
        }
        BoxStyle::Error => {
            println!("{} ❤️", indent_s);
            println!("{}{}", indent_s, box_top(width).bright_red());
            println!(
                "{}{}",
                indent_s,
                box_row("║", title, "║", width).bright_red().bold()
            );
            if !subtitle.is_empty() {
                println!("{}{}", indent_s, box_row("║", subtitle, "║", width).red());
            }
            println!("{}{}", indent_s, box_bottom(width).bright_red());
        }
        BoxStyle::Cyan => {
            println!("{} 💙", indent_s);
            println!("{}{}", indent_s, box_top(width).bright_cyan());
            println!(
                "{}{}",
                indent_s,
                box_row("║", title, "║", width).bright_cyan().bold()
            );
            if !subtitle.is_empty() {
                println!("{}{}", indent_s, box_row("║", subtitle, "║", width).cyan());
            }
            println!("{}{}", indent_s, box_bottom(width).bright_cyan());
        }
        BoxStyle::Magenta => {
            println!("{} 💜", indent_s);
            println!("{}{}", indent_s, box_top(width).bright_magenta());
            println!(
                "{}{}",
                indent_s,
                box_row("║", title, "║", width).bright_magenta().bold()
            );
            if !subtitle.is_empty() {
                println!(
                    "{}{}",
                    indent_s,
                    box_row("║", subtitle, "║", width).magenta()
                );
            }
            println!("{}{}", indent_s, box_bottom(width).bright_magenta());
        }
    }
}

pub fn print_item(item: &str, status: ItemStatus) {
    use colored::Colorize;

    let icon = status.icon();
    let indent_s = " ".repeat(indent() + 2);

    match status {
        ItemStatus::Created | ItemStatus::Pass => {
            println!(
                "{} {} {}",
                indent_s,
                icon.bright_green(),
                item.bright_white()
            );
        }
        ItemStatus::Updated | ItemStatus::Preserved => {
            println!(
                "{} {} {}",
                indent_s,
                icon.bright_yellow(),
                item.bright_white()
            );
        }
        ItemStatus::Unchanged | ItemStatus::Skipped => {
            println!(
                "{} {} {}",
                indent_s,
                icon.bright_black(),
                item.bright_white()
            );
        }
        ItemStatus::Failed | ItemStatus::Fail => {
            println!("{} {} {}", indent_s, icon.bright_red(), item.bright_white());
        }
        ItemStatus::Info => {
            println!("{} {} {}", indent_s, icon.cyan(), item.bright_white());
        }
    }
}

pub fn print_items_grid(items: &[(&str, ItemStatus)], cols: usize) {
    use colored::Colorize;

    let width = effective_width();
    let indent_s = " ".repeat(indent() + 2);
    let available = width.saturating_sub(4);
    let col_width = (available / cols).max(20);

    let mut col_pos = 0;
    for (item, status) in items {
        let icon = status.icon();
        let line = format!("{} {}", icon, item);
        let formatted = match status {
            ItemStatus::Created | ItemStatus::Pass => line.bright_green(),
            ItemStatus::Updated | ItemStatus::Preserved => line.bright_yellow(),
            ItemStatus::Unchanged | ItemStatus::Skipped => line.bright_black(),
            ItemStatus::Failed | ItemStatus::Fail => line.bright_red(),
            ItemStatus::Info => line.cyan(),
        };

        print!("{}{:<width$}", indent_s, formatted, width = col_width);

        col_pos += 1;
        if col_pos >= cols {
            println!();
            col_pos = 0;
        }
    }
    if col_pos > 0 {
        println!();
    }
}

pub fn print_section(title: &str) {
    use colored::Colorize;
    let indent_s = " ".repeat(indent() + 2);
    println!();
    println!("{}{}", indent_s, title.bold());
}

pub fn print_status_line(message: &str, status: ItemStatus) {
    use colored::Colorize;

    let icon = status.icon();
    let indent_s = " ".repeat(indent() + 2);

    match status {
        ItemStatus::Created | ItemStatus::Pass => {
            println!(
                "{}{} {}",
                indent_s,
                icon.bright_green(),
                message.bright_white()
            );
        }
        ItemStatus::Updated | ItemStatus::Preserved => {
            println!(
                "{}{} {}",
                indent_s,
                icon.bright_yellow(),
                message.bright_white()
            );
        }
        ItemStatus::Unchanged | ItemStatus::Skipped => {
            println!(
                "{}{} {}",
                indent_s,
                icon.bright_black(),
                message.bright_white()
            );
        }
        ItemStatus::Failed | ItemStatus::Fail => {
            println!(
                "{}{} {}",
                indent_s,
                icon.bright_red(),
                message.bright_white()
            );
        }
        ItemStatus::Info => {
            println!("{}{} {}", indent_s, icon.cyan(), message.bright_white());
        }
    }
}

pub fn print_summary(pass: usize, fail: usize) {
    use colored::Colorize;

    let width = effective_width();
    let indent_s = " ".repeat(indent());

    println!();
    println!("{}{}", indent_s, "📊".bold());
    println!("{}{}", indent_s, box_top(width));
    println!("{}{}", indent_s, box_row("║", "RESULTS", "║", width).bold());
    println!("{}{}", indent_s, box_bottom(width));
    println!();

    let indent_s2 = " ".repeat(indent() + 2);
    let pass_str = format!("{}", pass);
    let fail_str = format!("{}", fail);
    let total_str = format!("{}", pass + fail);

    if fail == 0 {
        println!("{}  {} All checks passed!", indent_s2, "✅".bright_green());
    } else {
        println!("{}  {} PASS", indent_s2, "✅".bright_green());
    }
    println!("{}{:>6}  {}", indent_s2, pass_str, "✅".bright_green());
    if fail > 0 {
        println!("{}{:>6}  {}", indent_s2, fail_str, "❌".bright_red());
    }
    println!("{}{:>6}  total", indent_s2, total_str);
}

pub fn print_mini_status(pass: usize, fail: usize, warn: usize) {
    use colored::Colorize;

    let indent_s = " ".repeat(indent() + 2);

    if fail > 0 {
        print!("{}{} ", indent_s, "❌".bright_red());
        print!("{} ", fail);
        print!("{} ", "FAIL".red());
    }
    if pass > 0 {
        if fail > 0 {
            print!("  ");
        }
        print!("{}{} ", indent_s, "✅".bright_green());
        print!("{} ", pass);
        print!("{} ", "PASS".green());
    }
    if warn > 0 {
        if fail > 0 || pass > 0 {
            print!("  ");
        }
        print!("{}{} ", indent_s, "⚠️".bright_yellow());
        print!("{} ", warn);
        print!("{}", "WARN".yellow());
    }
    println!();
}

pub fn print_list(items: &[&str]) {
    use colored::Colorize;
    let indent_s = " ".repeat(indent() + 2);
    for item in items {
        println!("{}  • {}", indent_s, item.bright_white());
    }
}
