use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ScrollMetrics {
    pub detail_max: usize,
    pub modal_max: usize,
    pub modal_offset: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum HitTarget {
    Page(char),
    Account(usize),
    Detail(usize),
    Field(usize),
    Submit,
    Cancel,
}

/// Hit regions from the latest completed render, including its real list and
/// table offsets. Invalidate after a terminal resize and redraw before reuse.
#[derive(Debug, Default)]
pub struct InteractionState {
    pub(super) frame: Option<Rect>,
    pub(super) account_area: Rect,
    pub(super) detail_area: Rect,
    pub(super) modal_area: Option<Rect>,
    pub(super) targets: Vec<(Rect, HitTarget)>,
    pub(super) metrics: ScrollMetrics,
    pub(super) modal_scroll: Option<usize>,
    pub(super) submit_label: Option<&'static str>,
    pub(super) account_offset: usize,
    pub(super) detail_offset: usize,
    page: Option<Page>,
    account: Option<AccountId>,
    mode: Option<std::mem::Discriminant<Mode>>,
    help_open: bool,
}

impl InteractionState {
    pub fn invalidate(&mut self) {
        self.frame = None;
        self.targets.clear();
        self.modal_area = None;
    }

    pub(super) fn begin(&mut self, area: Rect, app: &App) {
        let account = app.selected_account().map(|value| value.account().id());
        if self.page != Some(app.page) || self.account != account {
            self.detail_offset = 0;
        }
        self.invalidate();
        self.frame = Some(area);
        self.page = Some(app.page);
        self.account = account;
        self.mode = Some(std::mem::discriminant(&app.mode));
        self.help_open = app.help_open;
        self.metrics = ScrollMetrics::default();
        self.modal_scroll = app.modal_scroll;
        self.submit_label = match &app.mode {
            Mode::AccountForm(_)
            | Mode::TransactionForm(_)
            | Mode::TransferForm(_)
            | Mode::BudgetForm(_) => Some("Save"),
            Mode::SummaryReportForm(_) | Mode::TrendReportForm(_) | Mode::BudgetStatusForm(_) => {
                Some("Run")
            }
            _ => None,
        };
    }

    pub(super) fn target(&mut self, rect: Rect, target: HitTarget) {
        let Some(frame) = self.frame else { return };
        let rect = rect.intersection(frame);
        if rect.width > 0 && rect.height > 0 {
            self.targets.push((rect, target));
        }
    }

    pub(super) fn rows(
        &mut self,
        area: Rect,
        offset: usize,
        count: usize,
        height: u16,
        accounts: bool,
    ) {
        let mut y = area.y;
        for index in offset..count {
            if y >= area.bottom() || (accounts && area.bottom() - y < height) {
                break;
            }
            let row = Rect::new(area.x, y, area.width, height.min(area.bottom() - y));
            self.target(
                row,
                if accounts {
                    HitTarget::Account(index)
                } else {
                    HitTarget::Detail(index)
                },
            );
            y = y.saturating_add(height);
        }
    }

    fn matches(&self, app: &App) -> bool {
        self.frame.is_some()
            && self.page == Some(app.page)
            && self.account == app.selected_account().map(|value| value.account().id())
            && self.mode == Some(std::mem::discriminant(&app.mode))
            && self.help_open == app.help_open
    }
}

impl App {
    pub(super) fn page_display(&mut self, down: bool, modal: bool) {
        let metrics = self.scroll_metrics.get();
        if modal {
            self.modal_scroll = Some(move_offset(
                metrics.modal_offset,
                down,
                5,
                metrics.modal_max,
            ));
        } else if matches!(self.page, Page::Activity | Page::Reports) {
            self.detail_scroll = move_offset(self.detail_scroll, down, 5, metrics.detail_max);
        }
    }

    /// Translate only visible controls into existing actions. Clicks cannot
    /// bypass validation or confirm a destructive operation.
    pub fn handle_mouse(&mut self, event: MouseEvent, ui: &InteractionState) -> Action {
        if !ui.matches(self) {
            return Action::Continue;
        }
        let point = Position::new(event.column, event.row);
        if let MouseEventKind::ScrollUp | MouseEventKind::ScrollDown = event.kind {
            let down = event.kind == MouseEventKind::ScrollDown;
            if let Some(modal) = ui.modal_area {
                if modal.contains(point) {
                    self.modal_scroll = Some(move_offset(
                        ui.metrics.modal_offset,
                        down,
                        1,
                        ui.metrics.modal_max,
                    ));
                }
            } else if ui.account_area.contains(point) && !self.accounts.is_empty() {
                self.focus = Focus::Accounts;
                let index = move_offset(self.selected_account, down, 1, self.accounts.len() - 1);
                self.select_account_at(index);
            } else if ui.detail_area.contains(point) {
                if matches!(self.page, Page::Activity | Page::Reports) {
                    self.detail_scroll =
                        move_offset(self.detail_scroll, down, 1, ui.metrics.detail_max);
                } else if self.selectable_row_count() > 0 {
                    self.focus = Focus::Transactions;
                    self.selected_transaction = move_offset(
                        self.selected_transaction,
                        down,
                        1,
                        self.selectable_row_count() - 1,
                    );
                }
            }
            return Action::Continue;
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Action::Continue;
        }
        let Some(target) = ui
            .targets
            .iter()
            .rev()
            .find_map(|(rect, target)| rect.contains(point).then_some(*target))
        else {
            return Action::Continue;
        };
        match target {
            HitTarget::Page(key) => return self.handle_key(KeyCode::Char(key)),
            HitTarget::Account(index) => self.select_account_at(index),
            HitTarget::Detail(index) if index < self.selectable_row_count() => {
                self.focus = Focus::Transactions;
                self.selected_transaction = index;
            }
            HitTarget::Field(index) => self.focus_form_field(index),
            HitTarget::Submit => return self.handle_key(KeyCode::Enter),
            HitTarget::Cancel => return self.handle_key(KeyCode::Esc),
            HitTarget::Detail(_) => {}
        }
        Action::Continue
    }

    fn select_account_at(&mut self, index: usize) {
        if index >= self.accounts.len() {
            return;
        }
        self.focus = Focus::Accounts;
        if index != self.selected_account {
            self.selected_account = index;
            self.selected_transaction = 0;
            self.detail_scroll = 0;
            if self.page == Page::Budgets {
                self.budget = None;
            }
            if self.page == Page::Reports {
                self.report = None;
            }
        }
    }

    fn focus_form_field(&mut self, index: usize) {
        self.modal_scroll = None;
        match &mut self.mode {
            Mode::AccountForm(form) => {
                if let Some(field) = [AccountField::Name, AccountField::Currency].get(index) {
                    form.field = *field;
                }
            }
            Mode::TransactionForm(form) => {
                if let Some(field) = [
                    TransactionField::Kind,
                    TransactionField::Amount,
                    TransactionField::OccurredAt,
                    TransactionField::Description,
                    TransactionField::Category,
                ]
                .get(index)
                {
                    form.field = *field;
                }
            }
            Mode::TransferForm(form) => {
                if let Some(field) = [
                    TransferField::SourceAccount,
                    TransferField::DestinationAccount,
                    TransferField::SourceAmount,
                    TransferField::DestinationAmount,
                    TransferField::OccurredAt,
                    TransferField::Description,
                ]
                .get(index)
                {
                    form.field = *field;
                }
            }
            Mode::SummaryReportForm(form) => {
                if let Some(field) = [ReportField::From, ReportField::To].get(index) {
                    form.field = *field;
                }
            }
            Mode::TrendReportForm(form) => {
                if let Some(field) =
                    [ReportField::From, ReportField::To, ReportField::TimeZone].get(index)
                {
                    form.field = *field;
                }
            }
            Mode::BudgetForm(form) => {
                if let Some(field) = [
                    BudgetField::Category,
                    BudgetField::Month,
                    BudgetField::Limit,
                ]
                .get(index)
                {
                    form.field = *field;
                }
            }
            Mode::BudgetStatusForm(form) => {
                if let Some(field) =
                    [BudgetStatusField::Month, BudgetStatusField::TimeZone].get(index)
                {
                    form.field = *field;
                }
            }
            _ => {}
        }
    }
}

fn move_offset(current: usize, down: bool, amount: usize, max: usize) -> usize {
    let current = current.min(max);
    if down {
        current.saturating_add(amount).min(max)
    } else {
        current.saturating_sub(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository, InMemoryTransferRepository,
    };
    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    fn fixture(account_count: usize, transaction_count: usize) -> App {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        for index in 0..account_count {
            accounts
                .save(
                    Account::new(
                        AccountId::new(index as u64 + 1),
                        format!("Account {index:02}"),
                        Currency::Cny,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        for index in 0..transaction_count {
            transactions
                .save(
                    Transaction::new(
                        TransactionId::new(index as u64 + 1),
                        AccountId::new(1),
                        TransactionKind::Expense,
                        Money::from_minor_units(100 + index as i64, Currency::Cny),
                        "2026-08-30T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                        format!("Entry {index:02}"),
                        Category::Food,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        App::load(&accounts, &transactions, &transfers).unwrap()
    }

    fn draw(
        app: &App,
        ui: &mut InteractionState,
        width: u16,
        height: u16,
    ) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render_interactive(frame, app, ui))
            .unwrap();
        terminal
    }

    fn event(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn click(app: &mut App, ui: &InteractionState, rect: Rect) -> Action {
        app.handle_mouse(
            event(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
            ui,
        )
    }

    fn target(ui: &InteractionState, predicate: impl Fn(HitTarget) -> bool) -> Rect {
        ui.targets
            .iter()
            .find_map(|(rect, value)| predicate(*value).then_some(*rect))
            .expect("visible target")
    }

    fn focused_field(app: &App) -> usize {
        match &app.mode {
            Mode::AccountForm(form) => match form.field {
                AccountField::Name => 0,
                AccountField::Currency => 1,
            },
            Mode::TransactionForm(form) => match form.field {
                TransactionField::Kind => 0,
                TransactionField::Amount => 1,
                TransactionField::OccurredAt => 2,
                TransactionField::Description => 3,
                TransactionField::Category => 4,
            },
            Mode::TransferForm(form) => match form.field {
                TransferField::SourceAccount => 0,
                TransferField::DestinationAccount => 1,
                TransferField::SourceAmount => 2,
                TransferField::DestinationAmount => 3,
                TransferField::OccurredAt => 4,
                TransferField::Description => 5,
            },
            Mode::SummaryReportForm(form) => {
                if form.field == ReportField::From {
                    0
                } else {
                    1
                }
            }
            Mode::TrendReportForm(form) => match form.field {
                ReportField::From => 0,
                ReportField::To => 1,
                ReportField::TimeZone => 2,
            },
            Mode::BudgetForm(form) => match form.field {
                BudgetField::Category => 0,
                BudgetField::Month => 1,
                BudgetField::Limit => 2,
            },
            Mode::BudgetStatusForm(form) => match form.field {
                BudgetStatusField::Month => 0,
                BudgetStatusField::TimeZone => 1,
            },
            _ => panic!("expected form"),
        }
    }

    #[test]
    fn clicking_tabs_dispatches_existing_budget_request_and_invalidated_hits_are_ignored() {
        let mut app = fixture(2, 1);
        let mut ui = InteractionState::default();
        draw(&app, &mut ui, 80, 24);
        let budget = target(&ui, |value| matches!(value, HitTarget::Page('4')));
        assert_eq!(
            click(&mut app, &ui, budget),
            Action::RunBudget(BudgetRequest::List {
                account_id: AccountId::new(1)
            })
        );
        assert_eq!(app.page, Page::Budgets);
        draw(&app, &mut ui, 80, 24);
        let ledger = target(&ui, |value| matches!(value, HitTarget::Page('1')));
        ui.invalidate();
        assert_eq!(click(&mut app, &ui, ledger), Action::Continue);
        assert_eq!(app.page, Page::Budgets);
    }

    #[test]
    fn account_hits_use_scrolled_offset_and_ignore_odd_height_trailing_blank() {
        let mut app = fixture(20, 0);
        app.selected_account = 12;
        let mut ui = InteractionState::default();
        draw(&app, &mut ui, 80, 23);
        let (rect, index) = ui
            .targets
            .iter()
            .find_map(|(rect, target)| match target {
                HitTarget::Account(index) => Some((*rect, *index)),
                _ => None,
            })
            .unwrap();
        assert!(index > 0);
        click(&mut app, &ui, Rect::new(rect.x, rect.y + 1, 1, 1));
        assert_eq!(app.selected_account, index);
        draw(&app, &mut ui, 80, 23);
        assert_eq!(
            target(
                &ui,
                |value| matches!(value, HitTarget::Account(row) if row == index)
            )
            .y,
            rect.y
        );
        let selected = app.selected_account;
        assert!(ui.account_area.height % 2 == 1);
        let blank = Rect::new(ui.account_area.x, ui.account_area.bottom() - 1, 1, 1);
        click(&mut app, &ui, blank);
        assert_eq!(app.selected_account, selected);
    }

    #[test]
    fn transaction_hits_follow_compact_rows_scrolling_and_width_breakpoint() {
        let mut app = fixture(1, 30);
        app.focus = Focus::Transactions;
        let mut ui = InteractionState::default();
        for width in [80, 117, 118, 160] {
            app.selected_transaction = 25;
            draw(&app, &mut ui, width, 24);
            let (rect, index) = ui
                .targets
                .iter()
                .find_map(|(rect, target)| match target {
                    HitTarget::Detail(index) => Some((*rect, *index)),
                    _ => None,
                })
                .unwrap();
            assert!(index > 0);
            let expected = app.accounts[0].transactions[index].id();
            for line in 0..rect.height {
                click(&mut app, &ui, Rect::new(rect.x, rect.y + line, 1, 1));
                assert_eq!(app.selected_transaction().unwrap().id(), expected);
                draw(&app, &mut ui, width, 24);
                assert_eq!(
                    target(
                        &ui,
                        |value| matches!(value, HitTarget::Detail(row) if row == index)
                    )
                    .y,
                    rect.y
                );
            }
            assert_eq!(rect.height, if width < 118 { 3 } else { 1 });
        }
        let mut empty = fixture(1, 1);
        draw(&empty, &mut ui, 80, 24);
        for (x, y) in [(24, 5), (25, 4), (25, 18)] {
            click(&mut empty, &ui, Rect::new(x, y, 1, 1));
            assert_eq!(empty.focus, Focus::Accounts);
        }
    }

    #[test]
    fn each_form_field_maps_to_its_original_enum_and_modal_blocks_background() {
        for keys in [
            vec!['a'],
            vec!['n'],
            vec!['5', 'n'],
            vec!['3', 's'],
            vec!['3', 't'],
            vec!['4', 'b'],
            vec!['4', 'u'],
        ] {
            let mut app = fixture(2, 1);
            for key in keys {
                app.handle_key(KeyCode::Char(key));
            }
            let mut ui = InteractionState::default();
            draw(&app, &mut ui, 100, 30);
            let fields = ui
                .targets
                .iter()
                .filter_map(|(_, target)| match target {
                    HitTarget::Field(index) => Some(*index),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(!fields.is_empty());
            for index in fields {
                draw(&app, &mut ui, 100, 30);
                let rect = target(
                    &ui,
                    |value| matches!(value, HitTarget::Field(field) if field == index),
                );
                click(&mut app, &ui, rect);
                let selected = match &app.mode {
                    Mode::SummaryReportForm(form) => {
                        if form.field == ReportField::From {
                            0
                        } else {
                            1
                        }
                    }
                    Mode::TrendReportForm(form) => match form.field {
                        ReportField::From => 0,
                        ReportField::To => 1,
                        ReportField::TimeZone => 2,
                    },
                    _ => focused_field(&app),
                };
                assert_eq!(selected, index);
            }
            draw(&app, &mut ui, 100, 30);
            let page = app.page;
            click(&mut app, &ui, Rect::new(2, 1, 1, 1));
            app.handle_mouse(event(MouseEventKind::ScrollDown, 2, 5), &ui);
            assert_eq!(app.page, page);
            assert_eq!(app.selected_account, 0);
        }
    }

    #[test]
    fn submit_button_preserves_validation_and_cancel_does_not_submit() {
        let mut app = fixture(1, 0);
        app.handle_key(KeyCode::Char('n'));
        let mut ui = InteractionState::default();
        draw(&app, &mut ui, 80, 24);
        let submit = target(&ui, |value| matches!(value, HitTarget::Submit));
        assert_eq!(click(&mut app, &ui, submit), Action::Continue);
        assert!(matches!(&app.mode, Mode::TransactionForm(form) if form.error.is_some()));
        for character in "123".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        draw(&app, &mut ui, 80, 24);
        let description = target(&ui, |value| matches!(value, HitTarget::Field(3)));
        click(&mut app, &ui, description);
        for character in "Lunch".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        draw(&app, &mut ui, 80, 24);
        let submit = target(&ui, |value| matches!(value, HitTarget::Submit));
        let action = click(&mut app, &ui, submit);
        assert!(
            matches!(&action, Action::CreateTransaction(input) if input.amount_minor == "123"),
            "{action:?}; {:?}",
            app.mode
        );
        draw(&app, &mut ui, 80, 24);
        let cancel = target(&ui, |value| matches!(value, HitTarget::Cancel));
        assert_eq!(click(&mut app, &ui, cancel), Action::Continue);
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn help_is_modal_and_question_mark_remains_text_in_forms() {
        let mut app = fixture(2, 1);
        app.handle_key(KeyCode::Char('?'));
        let mut ui = InteractionState::default();
        let terminal = draw(&app, &mut ui, 80, 24);
        assert!(
            terminal
                .backend()
                .to_string()
                .contains("Keyboard & mouse help")
        );
        app.handle_key(KeyCode::Char('n'));
        click(&mut app, &ui, Rect::new(2, 1, 1, 1));
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.help_open);
        app.handle_key(KeyCode::Esc);
        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Char('?'));
        assert!(matches!(&app.mode, Mode::AccountForm(form) if form.name == "?"));
        assert!(!app.help_open);
    }

    #[test]
    fn read_only_activity_and_twelve_month_trend_remain_reachable() {
        let mut app = fixture(2, 30);
        let mut ui = InteractionState::default();
        app.handle_key(KeyCode::Char('2'));
        for _ in 0..8 {
            draw(&app, &mut ui, 80, 24);
            app.handle_key(KeyCode::PageDown);
        }
        let terminal = draw(&app, &mut ui, 80, 24);
        assert!(terminal.backend().to_string().contains("Entry 00"));
        assert_eq!(app.selected_account, 0);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.detail_scroll, 0);
        app.handle_key(KeyCode::Char('3'));
        let rows = (1..=12)
            .map(|month| MonthlyTrend {
                month: BudgetMonth::new(2026, month).unwrap(),
                summary: crate::domain::summary::calculate_summary(&app.accounts[1].account, &[])
                    .unwrap(),
            })
            .collect();
        app.set_report(ReportResult::Trend(rows));
        for _ in 0..4 {
            draw(&app, &mut ui, 80, 24);
            app.handle_key(KeyCode::PageDown);
        }
        let terminal = draw(&app, &mut ui, 80, 24);
        assert!(terminal.backend().to_string().contains("2026-12"));
        assert_eq!(app.detail_scroll, 11);
        app.handle_key(KeyCode::PageDown);
        assert_eq!(app.detail_scroll, 11);
        app.handle_mouse(
            event(MouseEventKind::ScrollUp, ui.detail_area.x, ui.detail_area.y),
            &ui,
        );
        assert_eq!(app.detail_scroll, 10);
    }

    #[test]
    fn short_dialog_keeps_focused_field_cancel_and_long_error_reachable() {
        let mut app = fixture(1, 0);
        app.handle_key(KeyCode::Char('n'));
        if let Mode::TransactionForm(form) = &mut app.mode {
            form.field = TransactionField::Category;
            form.error = Some(format!(
                "{}LAST_ERROR",
                "A long validation explanation. ".repeat(30)
            ));
        }
        let mut ui = InteractionState::default();
        let terminal = draw(&app, &mut ui, 80, 10);
        assert!(terminal.backend().to_string().contains("Category: Food"));
        assert!(terminal.backend().to_string().contains("[Esc] Cancel"));
        for _ in 0..20 {
            app.handle_key(KeyCode::PageDown);
            draw(&app, &mut ui, 80, 10);
        }
        let terminal = draw(&app, &mut ui, 80, 10);
        assert!(terminal.backend().to_string().contains("LAST_ERROR"));
        app.handle_key(KeyCode::Tab);
        let terminal = draw(&app, &mut ui, 80, 10);
        assert!(terminal.backend().to_string().contains("Kind: Expense"));
        let terminal = draw(&app, &mut ui, 20, 3);
        assert!(terminal.backend().to_string().contains("[Esc] Cancel"));
        assert!(
            ui.targets
                .iter()
                .any(|(_, value)| matches!(value, HitTarget::Cancel))
        );
        assert!(
            !ui.targets
                .iter()
                .any(|(_, value)| matches!(value, HitTarget::Submit))
        );
    }

    #[test]
    fn empty_active_field_label_is_visible_and_wrapped_field_remains_clickable() {
        let mut app = fixture(2, 0);
        let mut ui = InteractionState::default();
        app.handle_key(KeyCode::Char('n'));
        let terminal = draw(&app, &mut ui, 80, 24);
        let amount = target(&ui, |value| matches!(value, HitTarget::Field(1)));
        assert_eq!(terminal.backend().buffer()[(amount.x, amount.y)].fg, ACCENT);
        assert!(
            terminal.backend().buffer()[(amount.x, amount.y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        app.handle_key(KeyCode::Esc);
        app.handle_key(KeyCode::Char('a'));
        let terminal = draw(&app, &mut ui, 80, 24);
        let name = target(&ui, |value| matches!(value, HitTarget::Field(0)));
        assert_eq!(terminal.backend().buffer()[(name.x, name.y)].fg, ACCENT);
        app.handle_key(KeyCode::Esc);
        app.handle_key(KeyCode::Char('5'));
        app.handle_key(KeyCode::Char('n'));
        draw(&app, &mut ui, 30, 30);
        let wrapped = target(&ui, |value| matches!(value, HitTarget::Field(3)));
        assert!(wrapped.height > 1);
        click(
            &mut app,
            &ui,
            Rect::new(wrapped.x, wrapped.bottom() - 1, 1, 1),
        );
        assert!(matches!(
            app.mode,
            Mode::TransferForm(TransferForm {
                field: TransferField::DestinationAmount,
                ..
            })
        ));
    }

    #[test]
    fn selecting_another_account_clears_only_its_loaded_report_or_budget() {
        let mut app = fixture(2, 2);
        let mut ui = InteractionState::default();
        app.page = Page::Reports;
        app.report = Some(ReportResult::Category(vec![(
            Category::Food,
            Money::from_minor_units(1, Currency::Cny),
        )]));
        app.selected_transaction = 1;
        draw(&app, &mut ui, 80, 24);
        let second = target(&ui, |value| matches!(value, HitTarget::Account(1)));
        click(&mut app, &ui, second);
        assert!(app.report.is_none());
        assert_eq!(app.selected_transaction, 0);
        app.page = Page::Budgets;
        app.budget = Some(BudgetResult::List(vec![]));
        draw(&app, &mut ui, 80, 24);
        let first = target(&ui, |value| matches!(value, HitTarget::Account(0)));
        click(&mut app, &ui, first);
        assert!(app.budget.is_none());
        assert_eq!(app.selected_account, 0);
    }
}
