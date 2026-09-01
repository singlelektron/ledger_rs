use crate::domain::{
    account::AccountId,
    budget::BudgetMonth,
    money::{Currency, Money},
    transaction::{Category, TransactionKind},
};
use jiff::{civil::DateTime, tz::TimeZone};

use super::error::WebError;

pub(crate) fn parse_major_amount(value: &str) -> Option<i64> {
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.parse::<i64>().ok()?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()?.checked_mul(10)?,
        2 => fraction.parse::<i64>().ok()?,
        _ => return None,
    };
    let amount = whole.checked_mul(100)?.checked_add(fraction)?;
    (amount > 0).then_some(amount)
}

pub(crate) fn format_major_input(minor_units: i64) -> String {
    let absolute = minor_units.unsigned_abs();
    let sign = if minor_units < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", absolute / 100, absolute % 100)
}

pub(crate) fn parse_local_zoned(
    value: &str,
    time_zone_name: &str,
) -> Result<jiff::Zoned, WebError> {
    let local = value
        .parse::<DateTime>()
        .map_err(|_| WebError::bad_request("Enter a valid local date and time."))?;
    let time_zone = parse_time_zone(time_zone_name)?;
    time_zone
        .to_ambiguous_zoned(local)
        .unambiguous()
        .map_err(|_| WebError::bad_request("That local time is ambiguous or does not exist."))
}

/// Accepts either an IANA name (`Asia/Shanghai`) or a fixed UTC offset
/// (`+08:00`, `-05:00`, `+00`). Fixed offsets are what `edit_time_zone`
/// writes for imported timestamps that carry an offset but no IANA zone, so
/// saving an unchanged edit form cannot silently reinterpret the wall time in
/// another zone.
pub(crate) fn parse_time_zone(time_zone_name: &str) -> Result<TimeZone, WebError> {
    if let Some(offset_seconds) = parse_fixed_offset(time_zone_name) {
        return jiff::tz::Offset::from_seconds(offset_seconds)
            .map(jiff::tz::Offset::to_time_zone)
            .map_err(|_| WebError::bad_request("Enter a valid UTC offset."));
    }
    TimeZone::get(time_zone_name)
        .map_err(|_| WebError::bad_request("Enter a valid IANA time zone."))
}

/// Parses `+HH`, `+HH:MM`, or `+HH:MM:SS` (and their negative forms) into
/// offset seconds, or returns `None` for IANA names and other input.
pub(crate) fn parse_fixed_offset(value: &str) -> Option<i32> {
    let value = value.trim();
    let (sign, digits) = match value.as_bytes().first()? {
        b'+' => (1i32, &value[1..]),
        b'-' => (-1i32, &value[1..]),
        _ => return None,
    };
    let mut parts = digits.split(':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = parts.next().map_or(Ok(0), |part| part.parse()).ok()?;
    let seconds: i32 = parts.next().map_or(Ok(0), |part| part.parse()).ok()?;
    if parts.next().is_some() || hours > 23 || minutes > 59 || seconds > 59 {
        return None;
    }
    hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_add(seconds)?
        .checked_mul(sign)
}

/// The value shown in the edit form's time-zone field: the IANA name when the
/// stored timestamp has one, otherwise the exact fixed offset of the stored
/// instant (for example `+08:00`). `parse_time_zone` accepts both forms, so
/// re-saving an unchanged edit form reproduces the same instant instead of
/// falling back to a different zone.
pub(crate) fn edit_time_zone(zoned: &jiff::Zoned) -> String {
    match zoned.time_zone().iana_name() {
        Some(name) => name.to_owned(),
        None => zoned.offset().to_string(),
    }
}

pub(crate) fn parse_budget_month(value: &str) -> Result<BudgetMonth, WebError> {
    let (year, month) = value
        .split_once('-')
        .ok_or_else(|| WebError::bad_request("Enter a valid reporting month."))?;
    let year = year
        .parse::<i32>()
        .map_err(|_| WebError::bad_request("Enter a valid reporting year."))?;
    let month = month
        .parse::<u8>()
        .map_err(|_| WebError::bad_request("Enter a valid reporting month."))?;
    BudgetMonth::new(year, month)
        .map_err(|error| WebError::bad_request(format!("Invalid month: {error:?}")))
}

pub(crate) fn format_budget_month(month: BudgetMonth) -> String {
    format!("{:04}-{:02}", month.year(), month.month())
}

pub(crate) fn next_budget_month_for_report(month: BudgetMonth) -> Result<BudgetMonth, WebError> {
    let (year, month_number) = if month.month() == 12 {
        (month.year().checked_add(1), 1)
    } else {
        (Some(month.year()), month.month() + 1)
    };
    let year = year.ok_or_else(|| WebError::bad_request("Reporting range is too large."))?;
    BudgetMonth::new(year, month_number)
        .map_err(|error| WebError::bad_request(format!("Invalid reporting range: {error:?}")))
}

pub(crate) fn parse_transaction_kind(value: &str) -> Option<TransactionKind> {
    match value {
        "income" => Some(TransactionKind::Income),
        "expense" => Some(TransactionKind::Expense),
        "expense_refund" => Some(TransactionKind::ExpenseRefund),
        _ => None,
    }
}

pub(crate) fn parse_category(value: &str) -> Option<Category> {
    match value {
        "food" => Some(Category::Food),
        "transportation" => Some(Category::Transportation),
        "entertainment" => Some(Category::Entertainment),
        "necessary" => Some(Category::Necessary),
        "health" => Some(Category::Health),
        "education" => Some(Category::Education),
        "shopping" => Some(Category::Shopping),
        "travel" => Some(Category::Travel),
        "housing" => Some(Category::Housing),
        "salary" => Some(Category::Salary),
        "sale" => Some(Category::Sale),
        "family" => Some(Category::Family),
        "investment" => Some(Category::Investment),
        "other" => Some(Category::Other),
        _ => None,
    }
}

pub(crate) fn category_label(category: Category) -> &'static str {
    match category {
        Category::Food => "Food",
        Category::Transportation => "Transportation",
        Category::Entertainment => "Entertainment",
        Category::Necessary => "Necessary",
        Category::Health => "Health",
        Category::Education => "Education",
        Category::Shopping => "Shopping",
        Category::Travel => "Travel",
        Category::Housing => "Housing",
        Category::Salary => "Salary",
        Category::Sale => "Sale",
        Category::Family => "Family",
        Category::Investment => "Investment",
        Category::Other => "Other",
    }
}

pub(crate) fn category_options() -> String {
    category_options_selected(None, false)
}

pub(crate) fn category_options_selected(selected: Option<Category>, include_any: bool) -> String {
    let mut options = if include_any {
        String::from(r#"<option value="">Any category</option>"#)
    } else {
        String::new()
    };
    [
        Category::Food,
        Category::Transportation,
        Category::Entertainment,
        Category::Necessary,
        Category::Health,
        Category::Education,
        Category::Shopping,
        Category::Travel,
        Category::Housing,
        Category::Salary,
        Category::Sale,
        Category::Family,
        Category::Investment,
        Category::Other,
    ]
    .into_iter()
    .map(|category| {
        let value = category_label(category).to_ascii_lowercase();
        let selected_attribute = if selected == Some(category) {
            " selected"
        } else {
            ""
        };
        format!(
            r#"<option value="{}"{}>{}</option>"#,
            value,
            selected_attribute,
            category_label(category)
        )
    })
    .for_each(|option| options.push_str(&option));
    options
}

pub(crate) fn transaction_kind_options(
    selected: Option<TransactionKind>,
    include_any: bool,
) -> String {
    let mut options = if include_any {
        String::from(r#"<option value="">Any type</option>"#)
    } else {
        String::new()
    };
    for (kind, value, label) in [
        (TransactionKind::Expense, "expense", "Expense"),
        (TransactionKind::Income, "income", "Income"),
        (
            TransactionKind::ExpenseRefund,
            "expense_refund",
            "Expense refund",
        ),
    ] {
        let selected_attribute = if selected == Some(kind) {
            " selected"
        } else {
            ""
        };
        options.push_str(&format!(
            r#"<option value="{value}"{selected_attribute}>{label}</option>"#
        ));
    }
    options
}

pub(crate) fn parse_currency(value: &str) -> Option<Currency> {
    match value.to_ascii_uppercase().as_str() {
        "CNY" => Some(Currency::Cny),
        "USD" => Some(Currency::Usd),
        "EUR" => Some(Currency::Eur),
        "HKD" => Some(Currency::Hkd),
        "MYR" => Some(Currency::Myr),
        _ => None,
    }
}

pub(crate) fn currency_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Cny => "CNY",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
        Currency::Hkd => "HKD",
        Currency::Myr => "MYR",
    }
}

pub(crate) fn currency_options() -> String {
    ["CNY", "USD", "EUR", "HKD", "MYR"]
        .into_iter()
        .map(|code| format!(r#"<option value="{code}">{code}</option>"#))
        .collect::<Vec<_>>()
        .join("")
}

pub(crate) fn account_options(
    accounts: &[crate::domain::account::Account],
    excluded: Option<AccountId>,
    selected: Option<AccountId>,
) -> String {
    accounts
        .iter()
        .filter(|account| Some(account.id()) != excluded)
        .map(|account| {
            let selected_attribute = if Some(account.id()) == selected {
                " selected"
            } else {
                ""
            };
            format!(
                r#"<option value="{}"{}>{} · {}</option>"#,
                account.id().value(),
                selected_attribute,
                escape_html(account.name()),
                currency_code(account.currency()),
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

pub(crate) fn format_money(money: &Money) -> String {
    let amount = money.minor_units();
    let absolute = amount.unsigned_abs();
    let sign = if amount < 0 { "−" } else { "" };
    format!(
        "{sign}{}.{:02} {}",
        absolute / 100,
        absolute % 100,
        currency_code(money.currency())
    )
}

pub(crate) fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn page(title: &str, content: &str) -> String {
    let title = escape_html(title);
    let style = include_str!("../web_style.css");
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} · ledger_rs</title>
  <style>{style}</style>
</head>
<body>
  <header><nav><a class="brand" href="/"><span class="brand-mark">L</span><span>LEDGER<span class="brand-dim">_RS</span></span></a><div class="nav-links"><a href="/">Overview</a><a href="/reports">Reports</a><a href="/data">Data</a></div><span class="status"><i></i> LOCAL NODE</span></nav></header>
  <main>{content}</main>
</body>
</html>"#
    )
}
