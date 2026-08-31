use crate::{
    application::{
        account_balance::{GetAccountBalanceError, get_account_balance_with_transfers},
        list_accounts::{ListAccountsError, list_accounts},
        list_transactions::{ListTransactionsError, TransactionFilter, list_account_transactions},
        repository::{AccountRepository, TransactionRepository, TransferRepository},
    },
    domain::{
        account::{Account, AccountId},
        money::{Currency, Money},
        transaction::Transaction,
    },
};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    Accounts(ListAccountsError),
    Balance(GetAccountBalanceError),
    Transactions(ListTransactionsError),
}

impl From<ListAccountsError> for LoadError {
    fn from(error: ListAccountsError) -> Self {
        Self::Accounts(error)
    }
}

impl From<GetAccountBalanceError> for LoadError {
    fn from(error: GetAccountBalanceError) -> Self {
        Self::Balance(error)
    }
}

impl From<ListTransactionsError> for LoadError {
    fn from(error: ListTransactionsError) -> Self {
        Self::Transactions(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountOverview {
    account: Account,
    balance: Money,
    transactions: Vec<Transaction>,
}

impl AccountOverview {
    pub fn account(&self) -> &Account {
        &self.account
    }

    pub fn balance(&self) -> &Money {
        &self.balance
    }

    pub fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    accounts: Vec<AccountOverview>,
    selected_account: usize,
    selected_transaction: usize,
    focus: Focus,
    mode: Mode,
    status: Option<Status>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Continue,
    Reload,
    Quit,
    CreateAccount { name: String, currency: Currency },
    RenameAccount { id: AccountId, name: String },
    DeleteAccount { id: AccountId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Accounts,
    Transactions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Browse,
    AccountForm(AccountForm),
    ConfirmDeleteAccount(AccountId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccountFormKind {
    Create,
    Rename(AccountId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccountField {
    Name,
    Currency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AccountForm {
    kind: AccountFormKind,
    name: String,
    currency: Currency,
    field: AccountField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Status {
    message: String,
    is_error: bool,
}

impl App {
    pub fn load(
        account_repository: &impl AccountRepository,
        transaction_repository: &impl TransactionRepository,
        transfer_repository: &impl TransferRepository,
    ) -> Result<Self, LoadError> {
        let accounts = list_accounts(account_repository)?
            .into_iter()
            .map(|account| {
                let balance = get_account_balance_with_transfers(
                    account_repository,
                    transaction_repository,
                    transfer_repository,
                    account.id(),
                )?;
                let transactions = list_account_transactions(
                    account_repository,
                    transaction_repository,
                    account.id(),
                    TransactionFilter::default(),
                )?;
                Ok(AccountOverview {
                    account,
                    balance,
                    transactions,
                })
            })
            .collect::<Result<Vec<_>, LoadError>>()?;

        Ok(Self {
            accounts,
            selected_account: 0,
            selected_transaction: 0,
            focus: Focus::Accounts,
            mode: Mode::Browse,
            status: None,
        })
    }

    pub fn accounts(&self) -> &[AccountOverview] {
        &self.accounts
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.accounts.is_empty()).then_some(self.selected_account)
    }

    pub fn selected_account(&self) -> Option<&AccountOverview> {
        self.accounts.get(self.selected_account)
    }

    pub fn selected_transaction_index(&self) -> Option<usize> {
        self.selected_account()
            .filter(|account| !account.transactions().is_empty())
            .map(|_| self.selected_transaction)
    }

    pub fn selected_transaction(&self) -> Option<&Transaction> {
        self.selected_account()?
            .transactions()
            .get(self.selected_transaction)
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = Some(Status {
            message: message.into(),
            is_error,
        });
    }

    pub fn select_next(&mut self) {
        match self.focus {
            Focus::Accounts if !self.accounts.is_empty() => {
                self.selected_account = (self.selected_account + 1) % self.accounts.len();
                self.selected_transaction = 0;
            }
            Focus::Transactions => {
                if let Some(count) = self
                    .selected_account()
                    .map(|account| account.transactions().len())
                    .filter(|count| *count > 0)
                {
                    self.selected_transaction = (self.selected_transaction + 1) % count;
                }
            }
            Focus::Accounts => {}
        }
    }

    pub fn select_previous(&mut self) {
        match self.focus {
            Focus::Accounts if !self.accounts.is_empty() => {
                self.selected_account = self
                    .selected_account
                    .checked_sub(1)
                    .unwrap_or(self.accounts.len() - 1);
                self.selected_transaction = 0;
            }
            Focus::Transactions => {
                if let Some(count) = self
                    .selected_account()
                    .map(|account| account.transactions().len())
                    .filter(|count| *count > 0)
                {
                    self.selected_transaction = self
                        .selected_transaction
                        .checked_sub(1)
                        .unwrap_or(count - 1);
                }
            }
            Focus::Accounts => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) -> Action {
        match std::mem::replace(&mut self.mode, Mode::Browse) {
            Mode::AccountForm(form) => {
                let (mode, action) = handle_account_form_key(form, key);
                self.mode = mode;
                return action;
            }
            Mode::ConfirmDeleteAccount(id) => {
                return match key {
                    KeyCode::Char('y') | KeyCode::Char('Y') => Action::DeleteAccount { id },
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::Continue,
                    _ => {
                        self.mode = Mode::ConfirmDeleteAccount(id);
                        Action::Continue
                    }
                };
            }
            Mode::Browse => {}
        }

        match key {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char('r') => Action::Reload,
            KeyCode::Char('a') => {
                self.mode = Mode::AccountForm(AccountForm {
                    kind: AccountFormKind::Create,
                    name: String::new(),
                    currency: Currency::Cny,
                    field: AccountField::Name,
                });
                Action::Continue
            }
            KeyCode::Char('e') if self.focus == Focus::Accounts => {
                if let Some(account) = self.selected_account().map(AccountOverview::account) {
                    self.mode = Mode::AccountForm(AccountForm {
                        kind: AccountFormKind::Rename(account.id()),
                        name: account.name().to_string(),
                        currency: account.currency(),
                        field: AccountField::Name,
                    });
                }
                Action::Continue
            }
            KeyCode::Char('d') if self.focus == Focus::Accounts => {
                if let Some(id) = self
                    .selected_account()
                    .map(|account| account.account().id())
                {
                    self.mode = Mode::ConfirmDeleteAccount(id);
                }
                Action::Continue
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Accounts => Focus::Transactions,
                    Focus::Transactions => Focus::Accounts,
                };
                Action::Continue
            }
            KeyCode::Left => {
                self.focus = Focus::Accounts;
                Action::Continue
            }
            KeyCode::Right => {
                self.focus = Focus::Transactions;
                Action::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                Action::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
                Action::Continue
            }
            _ => Action::Continue,
        }
    }
}

fn handle_account_form_key(mut form: AccountForm, key: KeyCode) -> (Mode, Action) {
    let action = match key {
        KeyCode::Esc => return (Mode::Browse, Action::Continue),
        KeyCode::Enter => {
            let action = match form.kind {
                AccountFormKind::Create => Action::CreateAccount {
                    name: form.name,
                    currency: form.currency,
                },
                AccountFormKind::Rename(id) => Action::RenameAccount {
                    id,
                    name: form.name,
                },
            };
            return (Mode::Browse, action);
        }
        KeyCode::Tab if form.kind == AccountFormKind::Create => {
            form.field = match form.field {
                AccountField::Name => AccountField::Currency,
                AccountField::Currency => AccountField::Name,
            };
            Action::Continue
        }
        KeyCode::Left | KeyCode::Up if form.field == AccountField::Currency => {
            form.currency = previous_currency(form.currency);
            Action::Continue
        }
        KeyCode::Right | KeyCode::Down if form.field == AccountField::Currency => {
            form.currency = next_currency(form.currency);
            Action::Continue
        }
        KeyCode::Backspace if form.field == AccountField::Name => {
            form.name.pop();
            Action::Continue
        }
        KeyCode::Char(character) if form.field == AccountField::Name => {
            form.name.push(character);
            Action::Continue
        }
        _ => Action::Continue,
    };
    (Mode::AccountForm(form), action)
}

fn next_currency(currency: Currency) -> Currency {
    match currency {
        Currency::Cny => Currency::Usd,
        Currency::Usd => Currency::Eur,
        Currency::Eur => Currency::Hkd,
        Currency::Hkd => Currency::Myr,
        Currency::Myr => Currency::Cny,
    }
}

fn previous_currency(currency: Currency) -> Currency {
    match currency {
        Currency::Cny => Currency::Myr,
        Currency::Usd => Currency::Cny,
        Currency::Eur => Currency::Usd,
        Currency::Hkd => Currency::Eur,
        Currency::Myr => Currency::Hkd,
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    let [header_area, content_area, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .areas(frame.area());
    let [accounts_area, transactions_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .areas(content_area);

    let selected_name = app
        .selected_account()
        .map(|overview| overview.account().name())
        .unwrap_or("No account selected");
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" ledger_rs "),
            Span::styled(
                selected_name.to_string(),
                Style::default().fg(Color::Cyan).bold(),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title(" Dashboard ")),
        header_area,
    );

    render_accounts(frame, app, accounts_area);
    render_transactions(frame, app, transactions_area);
    render_footer(frame, app, footer_area);
    render_mode(frame, app);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![Line::from(
        "Tab/←/→ focus  ↑/k previous  ↓/j next  a add account  e edit  d delete  r refresh  q quit",
    )];
    if let Some(status) = &app.status {
        lines.push(Line::styled(
            status.message.clone(),
            Style::default().fg(if status.is_error {
                Color::Red
            } else {
                Color::Green
            }),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn render_mode(frame: &mut Frame, app: &App) {
    match &app.mode {
        Mode::Browse => {}
        Mode::AccountForm(form) => render_account_form(frame, form),
        Mode::ConfirmDeleteAccount(_) => {
            let area = centered_rect(frame.area(), 54, 5);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("Delete the selected account?"),
                    Line::from("Allowed only when it has no transactions, transfers, or budgets."),
                    Line::from("Press y to confirm or n/Esc to cancel."),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Confirm delete "),
                ),
                area,
            );
        }
    }
}

fn render_account_form(frame: &mut Frame, form: &AccountForm) {
    let is_create = form.kind == AccountFormKind::Create;
    let area = centered_rect(frame.area(), 58, if is_create { 8 } else { 7 });
    frame.render_widget(Clear, area);
    let mut lines = vec![Line::from(vec![
        Span::raw("Name: "),
        Span::styled(
            form.name.clone(),
            field_style(form.field == AccountField::Name),
        ),
    ])];
    if is_create {
        lines.push(Line::from(vec![
            Span::raw("Currency: "),
            Span::styled(
                currency_code(form.currency),
                field_style(form.field == AccountField::Currency),
            ),
        ]));
        lines.push(Line::from("Tab changes field; arrows change currency."));
    }
    lines.push(Line::from("Enter saves; Esc cancels."));
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(if is_create {
            " Create account "
        } else {
            " Rename account "
        })),
        area,
    );
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn field_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
    }
}

fn render_accounts(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items = if app.accounts().is_empty() {
        vec![ListItem::new("No accounts. Press a to create one.")]
    } else {
        app.accounts()
            .iter()
            .map(|overview| {
                ListItem::new(format!(
                    "{}  {}",
                    overview.account().name(),
                    format_money(overview.balance())
                ))
            })
            .collect()
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(focus_border(app.focus() == Focus::Accounts))
                .title(" Accounts "),
        )
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(app.selected_index());
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_transactions(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let Some(account) = app.selected_account() else {
        frame.render_widget(
            Paragraph::new("No transactions to display").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Transactions "),
            ),
            area,
        );
        return;
    };

    let rows = account.transactions().iter().map(|transaction| {
        Row::new(vec![
            Cell::from(transaction.occurred_at().to_string()),
            Cell::from(kind_label(transaction.kind())),
            Cell::from(format_transaction_amount(transaction)),
            Cell::from(transaction.description().to_string()),
        ])
    });
    let header = Row::new(["Occurred at", "Kind", "Amount", "Description"])
        .style(Style::default().fg(Color::Cyan).bold());
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(8),
            Constraint::Length(15),
            Constraint::Min(12),
        ],
    )
    .header(header)
    .column_spacing(1)
    .row_highlight_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ")
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(app.focus() == Focus::Transactions))
            .title(format!(" Transactions ({}) ", account.transactions().len())),
    );
    let mut state = TableState::default().with_selected(app.selected_transaction_index());
    frame.render_stateful_widget(table, area, &mut state);
}

fn focus_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn kind_label(kind: crate::domain::transaction::TransactionKind) -> &'static str {
    use crate::domain::transaction::TransactionKind;
    match kind {
        TransactionKind::Income => "Income",
        TransactionKind::Expense => "Expense",
        TransactionKind::ExpenseRefund => "Refund",
    }
}

fn format_transaction_amount(transaction: &Transaction) -> String {
    use crate::domain::transaction::TransactionKind;
    let sign = match transaction.kind() {
        TransactionKind::Income | TransactionKind::ExpenseRefund => "+",
        TransactionKind::Expense => "-",
    };
    format!("{sign}{}", format_money(transaction.amount()))
}

fn format_money(money: &Money) -> String {
    let absolute = money.minor_units().unsigned_abs();
    format!(
        "{}{}.{:02} {}",
        if money.minor_units().is_negative() {
            "-"
        } else {
            ""
        },
        absolute / 100,
        absolute % 100,
        currency_code(money.currency())
    )
}

fn currency_code(currency: crate::domain::money::Currency) -> &'static str {
    use crate::domain::money::Currency;
    match currency {
        Currency::Cny => "CNY",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
        Currency::Hkd => "HKD",
        Currency::Myr => "MYR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::repository::{AccountRepository, TransactionRepository, TransferRepository},
        domain::{
            account::{Account, AccountId},
            money::{Currency, Money},
            transaction::{Category, Transaction, TransactionId, TransactionKind},
            transfer::NewTransfer,
        },
        infrastructure::in_memory::{
            InMemoryAccountRepository, InMemoryTransactionRepository, InMemoryTransferRepository,
        },
    };
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn loads_accounts_balances_and_newest_first_transactions() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let mut transfers = InMemoryTransferRepository::new();
        let cash = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let bank = Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        accounts.save(bank.clone()).unwrap();
        accounts.save(cash.clone()).unwrap();
        transactions
            .save(
                Transaction::new(
                    TransactionId::new(1),
                    cash.id(),
                    TransactionKind::Income,
                    Money::from_minor_units(1_000, Currency::Cny),
                    "2026-08-30T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Salary".to_string(),
                    Category::Salary,
                )
                .unwrap(),
            )
            .unwrap();
        transactions
            .save(
                Transaction::new(
                    TransactionId::new(2),
                    cash.id(),
                    TransactionKind::Expense,
                    Money::from_minor_units(250, Currency::Cny),
                    "2026-08-31T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Lunch".to_string(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap();
        transfers
            .create(
                NewTransfer::new(
                    cash.id(),
                    bank.id(),
                    Money::from_minor_units(100, Currency::Cny),
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-31T11:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Move money".to_string(),
                )
                .unwrap(),
            )
            .unwrap();

        let app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(app.accounts()[0].account(), &cash);
        assert_eq!(
            app.accounts()[0].balance(),
            &Money::from_minor_units(650, Currency::Cny)
        );
        assert_eq!(app.accounts()[0].transactions()[0].id().value(), 2);
        assert_eq!(
            app.accounts()[1].balance(),
            &Money::from_minor_units(100, Currency::Cny)
        );
    }

    #[test]
    fn selection_wraps_and_empty_state_is_safe() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut empty = App::load(&accounts, &transactions, &transfers).unwrap();
        empty.select_next();
        empty.select_previous();
        assert_eq!(empty.selected_index(), None);

        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        accounts
            .save(Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.select_previous();
        assert_eq!(app.selected_index(), Some(1));
        app.select_next();
        assert_eq!(app.selected_index(), Some(0));

        app.handle_key(KeyCode::Right);
        assert_eq!(app.focus(), Focus::Transactions);
        app.select_next();
        assert_eq!(app.selected_transaction_index(), None);
    }

    #[test]
    fn key_bindings_navigate_reload_and_quit() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        accounts
            .save(Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(app.handle_key(KeyCode::Char('j')), Action::Continue);
        assert_eq!(app.selected_index(), Some(1));
        assert_eq!(app.handle_key(KeyCode::Up), Action::Continue);
        assert_eq!(app.selected_index(), Some(0));
        assert_eq!(app.handle_key(KeyCode::Char('r')), Action::Reload);
        assert_eq!(app.handle_key(KeyCode::Esc), Action::Quit);
    }

    #[test]
    fn selects_transactions_independently_from_accounts() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        for id in 1..=2 {
            transactions
                .save(
                    Transaction::new(
                        TransactionId::new(id),
                        account.id(),
                        TransactionKind::Expense,
                        Money::from_minor_units(100, Currency::Cny),
                        format!("2026-08-30T10:00:0{id}+08:00[Asia/Shanghai]")
                            .parse()
                            .unwrap(),
                        format!("Expense {id}"),
                        Category::Food,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(app.handle_key(KeyCode::Tab), Action::Continue);
        assert_eq!(app.focus(), Focus::Transactions);
        assert_eq!(app.selected_transaction().unwrap().id().value(), 2);
        app.select_next();
        assert_eq!(app.selected_transaction().unwrap().id().value(), 1);
        app.select_next();
        assert_eq!(app.selected_transaction().unwrap().id().value(), 2);
        assert_eq!(app.selected_index(), Some(0));
    }

    #[test]
    fn renders_empty_dashboard_and_loaded_account() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let empty = App::load(&accounts, &transactions, &transfers).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        terminal.draw(|frame| render(frame, &empty)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("No accounts"));

        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let loaded = App::load(&accounts, &transactions, &transfers).unwrap();
        terminal.draw(|frame| render(frame, &loaded)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("Cash"));
        assert!(screen.contains("0.00 CNY"));
        assert!(screen.contains("Transactions (0)"));
    }

    #[test]
    fn account_forms_emit_create_rename_and_delete_actions() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut empty = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(empty.handle_key(KeyCode::Char('a')), Action::Continue);
        for character in "Cash".chars() {
            assert_eq!(empty.handle_key(KeyCode::Char(character)), Action::Continue);
        }
        empty.handle_key(KeyCode::Tab);
        empty.handle_key(KeyCode::Right);
        assert_eq!(
            empty.handle_key(KeyCode::Enter),
            Action::CreateAccount {
                name: "Cash".to_string(),
                currency: Currency::Usd,
            }
        );

        let account = Account::new(AccountId::new(7), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Char('e'));
        for _ in 0..4 {
            app.handle_key(KeyCode::Backspace);
        }
        for character in "Wallet".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        assert_eq!(
            app.handle_key(KeyCode::Enter),
            Action::RenameAccount {
                id: account.id(),
                name: "Wallet".to_string(),
            }
        );

        app.handle_key(KeyCode::Char('d'));
        assert_eq!(
            app.handle_key(KeyCode::Char('y')),
            Action::DeleteAccount { id: account.id() }
        );
    }

    #[test]
    fn renders_account_dialog_and_operation_status() {
        let accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.set_status("Create failed: empty name", true);
        app.handle_key(KeyCode::Char('a'));
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let screen = terminal.backend().to_string();
        assert!(screen.contains("Create account"));
        assert!(screen.contains("Currency: CNY"));
        assert!(screen.contains("Create failed: empty name"));
    }
}
