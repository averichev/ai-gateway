use comfy_table::{
    Attribute, Cell, Color, ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS,
    presets::UTF8_FULL,
};
use owo_colors::OwoColorize;
use std::env;

#[derive(Debug, Clone, Copy)]
pub enum CheckState {
    Ok,
    Warn,
    Fail,
    Info,
}

pub fn print_banner() {
    println!();
    println!("{}", "AI Gateway ctl".bold().bright_cyan());
    println!("{}", "Управление production-сервером ai-gateway".dimmed());
    println!();
}

pub fn print_section(title: &str) {
    println!("{}", format!("== {title}").bold().bright_cyan());
}

pub fn print_success(message: &str) {
    println!("{} {}", "✓".bright_green().bold(), message.bright_green());
}

pub fn print_warn(message: &str) {
    println!("{} {}", "!".bright_yellow().bold(), message.yellow());
}

pub fn print_info(message: &str) {
    println!("{} {}", "•".bright_cyan().bold(), message);
}

pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".bright_red().bold(), message.bright_red());
}

pub fn print_key_value_table(title: &str, rows: &[(String, String)]) {
    print_section(title);

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(table_width())
        .set_header(vec![
            Cell::new("Параметр")
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold),
            Cell::new("Значение")
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold),
        ]);

    for (key, value) in rows {
        table.add_row(vec![Cell::new(key), Cell::new(value)]);
    }

    println!("{table}");
    println!();
}

pub fn print_check_table(title: &str, rows: &[(String, CheckState, String)]) {
    print_section(title);

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(table_width())
        .set_header(vec![
            Cell::new("Проверка")
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold),
            Cell::new("Статус")
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold),
            Cell::new("Подробности")
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold),
        ]);

    for (check, state, details) in rows {
        let (label, color) = match state {
            CheckState::Ok => ("OK", Color::Green),
            CheckState::Warn => ("ВНИМ", Color::Yellow),
            CheckState::Fail => ("ОШИБ", Color::Red),
            CheckState::Info => ("ИНФО", Color::Blue),
        };

        table.add_row(vec![
            Cell::new(check),
            Cell::new(label).fg(color).add_attribute(Attribute::Bold),
            Cell::new(details),
        ]);
    }

    println!("{table}");
    println!();
}

fn table_width() -> u16 {
    env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| *value >= 80)
        .unwrap_or(120)
}
