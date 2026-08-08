//! Debt settlement: charging a player, and the liquidation popup that lets them
//! mortgage and sell down before bankruptcy is forced on them.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, HOTEL, Modal};
use crate::ui::{Cursor, choice_popup};

/// Who receives a settled debt.
pub(super) enum Payee {
    Bank,
    Player(usize),
    /// Split evenly among these players.
    Split(Vec<usize>),
}

impl Payee {
    /// The player who inherits the debtor's estate on bankruptcy, if any.
    fn creditor(&self) -> Option<usize> {
        match self {
            Payee::Player(p) => Some(*p),
            _ => None,
        }
    }
}

/// A debt the player can't cover in cash but could cover by liquidating.
pub(super) struct Debt {
    who: usize,
    amount: u32,
    payee: Payee,
    /// Steps to move once the debt clears (the forced-bail exit from jail).
    then_advance: Option<usize>,
    slots: Vec<usize>,
    cursor: Cursor,
}

impl Game {
    pub(super) fn charge(&mut self, who: usize, amount: u32, payee: Payee) {
        self.charge_then(who, amount, payee, None);
    }

    /// Bill `who`. Paid outright when the cash is there; otherwise they either
    /// liquidate (popup) or, if their whole estate falls short, go bankrupt.
    pub(super) fn charge_then(
        &mut self,
        who: usize,
        amount: u32,
        payee: Payee,
        then_advance: Option<usize>,
    ) {
        if amount == 0 {
            return;
        }
        if self.players[who].money >= amount {
            self.settle(who, amount, &payee);
            if let Some(steps) = then_advance {
                self.advance(who, steps);
            }
            return;
        }
        if self.net_worth(who) < amount {
            let left = self.players[who].money;
            let creditor = payee.creditor();
            self.settle(who, left, &payee);
            self.bankrupt(who, creditor);
            return;
        }
        let slots = self.holdings(who);
        self.notify(
            format!("Player {} owes ${amount} — raise the cash or fold", who + 1),
            Level::Error,
        );
        self.modal = Modal::Debt(Debt {
            who,
            amount,
            payee,
            then_advance,
            cursor: Cursor::new(slots.len()),
            slots,
        });
    }

    /// Move `amount` (capped at what they hold) from `who` to `payee`.
    fn settle(&mut self, who: usize, amount: u32, payee: &Payee) {
        let amount = amount.min(self.players[who].money);
        self.players[who].money -= amount;
        match payee {
            Payee::Bank => {
                self.notify(format!("Player {} paid ${amount}", who + 1), Level::Warn);
            }
            Payee::Player(p) => {
                self.players[*p].money += amount;
                self.notify(
                    format!("Player {} paid ${amount} to Player {}", who + 1, p + 1),
                    Level::Warn,
                );
            }
            Payee::Split(others) => {
                let per = amount / others.len().max(1) as u32;
                let mut left = amount;
                for &i in others {
                    let paid = left.min(per);
                    self.players[i].money += paid;
                    left -= paid;
                }
                self.notify(format!("Player {} paid ${per} to each player", who + 1), Level::Warn);
            }
        }
    }

    /// Cash plus everything the estate could be liquidated for: mortgage value
    /// on unmortgaged holdings, half the build cost on every house.
    fn net_worth(&self, who: usize) -> u32 {
        self.estate(who)
            .map(|s| {
                let mortgage = if s.is_mortgaged() { 0 } else { s.mortgage_value() };
                mortgage + u32::from(s.houses()) * s.house_refund()
            })
            .sum::<u32>()
            + self.players[who].money
    }

    /// Handle one liquidation key press. Returns `true` to keep the popup open;
    /// the debt has no escape key, so it closes only on payment or bankruptcy.
    pub(super) fn debt_input(&mut self, debt: &mut Debt, key: KeyCode) -> bool {
        let idx = debt.slots[debt.cursor.selected];
        match key {
            KeyCode::Up => debt.cursor.up(),
            KeyCode::Down => debt.cursor.down(),
            KeyCode::Enter => self.mortgage(debt.who, idx),
            KeyCode::Char('s') => self.sell_house(debt.who, idx),
            KeyCode::Char('b') => {
                let (left, creditor) = (self.players[debt.who].money, debt.payee.creditor());
                self.settle(debt.who, left, &debt.payee);
                self.bankrupt(debt.who, creditor);
                self.settle_if_bankrupt(debt.who);
                self.run_pending();
                return false;
            }
            _ => {}
        }
        if self.players[debt.who].money < debt.amount {
            return true;
        }
        self.settle(debt.who, debt.amount, &debt.payee);
        if let Some(steps) = debt.then_advance {
            self.advance(debt.who, steps);
        }
        self.run_pending();
        false
    }

    pub(super) fn render_debt(&self, frame: &mut Frame, debt: &Debt) {
        let title = format!(
            " Player {} owes ${} — cash ${} — [enter] mortgage  [s] sell  [b] bankrupt ",
            debt.who + 1,
            debt.amount,
            self.players[debt.who].money
        );
        let lines: Vec<String> = debt
            .slots
            .iter()
            .map(|&i| {
                let s = &self.board[i];
                let refund = s.house_refund();
                match s.houses() {
                    0 if s.is_mortgaged() => format!("{}  [mortgaged]", s.name()),
                    0 => format!("{}  [mortgage +${}]", s.name(), s.mortgage_value()),
                    HOTEL => format!("{}  hotel  [sell +${refund}]", s.name()),
                    h => format!("{}  {h} house  [sell +${refund}]", s.name()),
                }
            })
            .collect();
        choice_popup(frame, &title, &lines, debt.cursor.selected);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testkit::*;
    use crate::space::ColorGroup;

    /// Player 0 holds $10 and the two brown streets ($30 of mortgage value
    /// each), then lands on player 1's Boardwalk for $50 of rent.
    fn short_on_rent() -> Game {
        let mut g = game(2, 1500);
        g.players[0].money = 10;
        g.players[1].money = 0;
        own(&mut g, BOARDWALK, 1);
        own(&mut g, MEDITERRANEAN, 0);
        own(&mut g, BALTIC, 0);
        place(&mut g, 0, BOARDWALK - 5);
        g.apply_roll(2, 3);
        g
    }

    #[test]
    fn affordable_debts_are_paid_outright() {
        let mut g = game(2, 1500);
        g.charge(0, 200, Payee::Player(1));
        assert!(matches!(g.modal, Modal::None), "no popup when the cash is there");
        assert_eq!(g.players[0].money, 1300);
        assert_eq!(g.players[1].money, 1700);
    }

    #[test]
    fn a_zero_debt_is_a_no_op() {
        let mut g = game(2, 1500);
        g.charge(0, 0, Payee::Bank);
        assert_eq!(g.players[0].money, 1500);
        assert!(matches!(g.modal, Modal::None));
    }

    #[test]
    fn a_split_debt_is_divided_evenly() {
        let mut g = game(3, 1500);
        g.charge(0, 100, Payee::Split(vec![1, 2]));
        assert_eq!(g.players[0].money, 1400);
        assert_eq!(g.players[1].money, 1550);
        assert_eq!(g.players[2].money, 1550);
    }

    #[test]
    fn an_unaffordable_debt_opens_liquidation() {
        let g = short_on_rent();
        assert!(matches!(g.modal, Modal::Debt(_)), "should offer liquidation");
        assert!(!g.players[0].bankrupt);
        assert_eq!(g.players[0].money, 10, "nothing is paid until the debt clears");
        assert_eq!(g.players[1].money, 0);
    }

    #[test]
    fn mortgaging_enough_settles_the_debt() {
        let mut g = short_on_rent();
        g.handle_key(KeyCode::Enter); // mortgage Mediterranean (+$30)
        assert!(matches!(g.modal, Modal::Debt(_)), "still $10 short of $50");
        g.handle_key(KeyCode::Down);
        g.handle_key(KeyCode::Enter); // mortgage Baltic (+$30)

        assert!(!matches!(g.modal, Modal::Debt(_)), "debt cleared");
        assert_eq!(g.players[0].money, 20, "10 + 30 + 30 - 50");
        assert_eq!(g.players[1].money, 50);
        assert!(!g.players[0].bankrupt);
    }

    #[test]
    fn selling_houses_also_raises_the_cash() {
        let mut g = game(2, 1500);
        g.players[0].money = 0;
        g.players[1].money = 0;
        own_group(&mut g, ColorGroup::Brown, 0);
        set_houses(&mut g, MEDITERRANEAN, 1);
        set_houses(&mut g, BALTIC, 1);
        own(&mut g, BOARDWALK, 1);
        place(&mut g, 0, BOARDWALK - 5);
        g.apply_roll(2, 3);

        assert!(matches!(g.modal, Modal::Debt(_)));
        g.handle_key(KeyCode::Char('s')); // sell a house off Mediterranean (+$25)
        g.handle_key(KeyCode::Down);
        g.handle_key(KeyCode::Char('s')); // sell a house off Baltic (+$25)
        assert_eq!(g.board[MEDITERRANEAN].houses(), 0);
        assert_eq!(g.players[0].money, 0, "50 raised, 50 paid");
        assert_eq!(g.players[1].money, 50);
    }

    #[test]
    fn a_hopeless_debt_bankrupts_immediately() {
        let mut g = game(2, 1500);
        g.players[0].money = 10;
        g.players[1].money = 0;
        own(&mut g, BOARDWALK, 1);
        place(&mut g, 0, BOARDWALK - 5);
        g.apply_roll(2, 3);

        assert!(g.players[0].bankrupt, "no estate to sell");
        assert_eq!(g.players[1].money, 10, "the creditor takes the remaining cash");
    }

    #[test]
    fn folding_hands_the_estate_to_the_creditor() {
        let mut g = short_on_rent();
        g.handle_key(KeyCode::Char('b'));
        assert!(g.players[0].bankrupt);
        assert_eq!(g.board[MEDITERRANEAN].owner(), Some(1));
        assert_eq!(g.board[BALTIC].owner(), Some(1));
        assert_eq!(g.players[1].money, 10, "and the cash that was left");
    }

    #[test]
    fn folding_on_a_bank_debt_returns_the_estate_to_the_bank() {
        let mut g = game(2, 1500);
        g.players[0].money = 10;
        own(&mut g, MEDITERRANEAN, 0);
        own(&mut g, BALTIC, 0);
        g.charge(0, 50, Payee::Bank);
        assert!(matches!(g.modal, Modal::Debt(_)));

        g.handle_key(KeyCode::Char('b'));
        assert_eq!(g.board[MEDITERRANEAN].owner(), None);
        assert_eq!(g.board[BALTIC].owner(), None);
    }

    #[test]
    fn a_debt_matching_net_worth_exactly_is_survivable() {
        let mut g = game(2, 1500);
        g.players[0].money = 10;
        own(&mut g, MEDITERRANEAN, 0);
        own(&mut g, BALTIC, 0);
        g.charge(0, 70, Payee::Bank); // exactly cash + both mortgage values
        assert!(matches!(g.modal, Modal::Debt(_)));

        g.handle_key(KeyCode::Enter);
        g.handle_key(KeyCode::Down);
        g.handle_key(KeyCode::Enter);
        assert!(!g.players[0].bankrupt);
        assert_eq!(g.players[0].money, 0);
    }

    #[test]
    fn a_debt_one_dollar_past_net_worth_is_fatal() {
        let mut g = game(2, 1500);
        g.players[0].money = 10;
        own(&mut g, MEDITERRANEAN, 0);
        own(&mut g, BALTIC, 0);
        g.charge(0, 71, Payee::Bank);
        assert!(g.players[0].bankrupt, "no popup when the estate cannot cover it");
    }

    #[test]
    fn liquidation_has_no_escape_key() {
        let mut g = short_on_rent();
        g.handle_key(KeyCode::Esc);
        assert!(matches!(g.modal, Modal::Debt(_)), "Esc must not dodge the debt");
    }

    #[test]
    fn net_worth_counts_cash_mortgages_and_houses() {
        let mut g = game(2, 100);
        own_group(&mut g, ColorGroup::Brown, 0);
        set_houses(&mut g, MEDITERRANEAN, 2);
        // 100 cash + 30 + 30 mortgage value + 2 houses at half of $50.
        assert_eq!(g.net_worth(0), 210);

        g.board[BALTIC].set_mortgaged(true);
        assert_eq!(g.net_worth(0), 180, "an already-mortgaged deed raises nothing");
    }

    #[test]
    fn a_deferred_move_runs_once_the_debt_clears() {
        let mut g = game(2, 1500);
        g.players[0].money = 10;
        own(&mut g, MEDITERRANEAN, 0);
        own(&mut g, BALTIC, 0);
        place(&mut g, 0, JAIL);
        g.charge_then(0, 50, Payee::Bank, Some(5));
        assert!(matches!(g.modal, Modal::Debt(_)));
        assert_eq!(g.players[0].position, JAIL, "held until the debt is settled");

        g.handle_key(KeyCode::Enter);
        g.handle_key(KeyCode::Down);
        g.handle_key(KeyCode::Enter);
        assert_eq!(g.players[0].money, 20);
        assert_eq!(g.players[0].position, JAIL + 5, "then the move happens");
    }
}
