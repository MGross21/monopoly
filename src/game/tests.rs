//! Tests for the turn loop, movement, buying, and elimination in `game/mod.rs`.

use super::testkit::*;
use super::*;

// --- movement ---------------------------------------------------------------

#[test]
fn advance_wraps_and_pays_the_go_salary() {
    let mut g = game(2, 1500);
    place(&mut g, 0, 38);
    g.advance(0, 4);
    assert_eq!(g.players[0].position, 2);
    assert_eq!(g.players[0].money, 1500 + GO_SALARY);
}

#[test]
fn landing_exactly_on_go_pays_the_salary() {
    let mut g = game(2, 1500);
    place(&mut g, 0, 36);
    g.advance(0, 4);
    assert_eq!(g.players[0].position, 0);
    assert_eq!(g.players[0].money, 1500 + GO_SALARY);
}

#[test]
fn a_move_that_does_not_pass_go_pays_nothing() {
    let mut g = game(2, 1500);
    place(&mut g, 0, 1);
    g.advance(0, 5);
    assert_eq!(g.players[0].position, 6);
    assert_eq!(g.players[0].money, 1500);
}

#[test]
fn go_to_jail_space_jails_without_the_salary() {
    let mut g = game(2, 1500);
    place(&mut g, 0, GO_TO_JAIL - 4);
    g.advance(0, 4);
    assert!(g.players[0].in_jail);
    assert_eq!(g.players[0].position, JAIL);
    assert_eq!(g.players[0].money, 1500);
}

#[test]
fn free_parking_pays_nothing() {
    let mut g = game(2, 1500);
    place(&mut g, 0, FREE_PARKING - 3);
    g.advance(0, 3);
    assert_eq!(g.players[0].position, FREE_PARKING);
    assert_eq!(g.players[0].money, 1500);
}

// --- doubles ----------------------------------------------------------------

#[test]
fn third_double_jails_without_moving() {
    let mut g = game(2, 1500);
    g.doubles = 2;
    place(&mut g, 0, 0);
    g.apply_roll(3, 3);
    assert!(g.players[0].in_jail);
    assert_eq!(g.players[0].position, JAIL, "must not resolve a landing first");
    assert_eq!(g.players[0].money, 1500, "no GO salary, no purchase");
    assert_eq!(g.doubles, 0);
    assert!(!g.can_roll);
}

#[test]
fn earlier_doubles_grant_a_reroll() {
    let mut g = game(2, 1500);
    g.apply_roll(2, 2);
    assert_eq!(g.doubles, 1);
    assert!(g.can_roll);
    g.apply_roll(1, 1);
    assert_eq!(g.doubles, 2);
    assert!(g.can_roll);
    assert!(!g.players[0].in_jail);
}

#[test]
fn a_non_double_ends_the_roll() {
    let mut g = game(2, 1500);
    g.apply_roll(2, 5);
    assert_eq!(g.doubles, 0);
    assert!(!g.can_roll);
    assert!(g.has_rolled);
}

#[test]
fn doubles_do_not_carry_across_turns() {
    let mut g = game(2, 1500);
    g.apply_roll(2, 2);
    g.end_turn();
    assert_eq!(g.doubles, 0);
    assert!(g.can_roll);
    assert!(!g.has_rolled);
}

#[test]
fn landing_in_jail_on_doubles_still_ends_the_turn() {
    let mut g = game(2, 1500);
    place(&mut g, 0, GO_TO_JAIL - 6);
    g.apply_roll(3, 3);
    assert!(g.players[0].in_jail);
    assert!(!g.can_roll, "no bonus roll out of jail");
}

// --- rent in play (the tables themselves live in rent.rs) --------------------

#[test]
fn landing_on_a_rival_street_transfers_the_rent() {
    let mut g = game(2, 1500);
    own(&mut g, BOARDWALK, 1);
    place(&mut g, 0, BOARDWALK - 5);
    g.apply_roll(2, 3);
    assert_eq!(g.players[0].money, 1450);
    assert_eq!(g.players[1].money, 1550);
}

#[test]
fn landing_on_your_own_street_costs_nothing() {
    let mut g = game(2, 1500);
    own(&mut g, BOARDWALK, 0);
    place(&mut g, 0, BOARDWALK - 5);
    g.apply_roll(2, 3);
    assert_eq!(g.players[0].money, 1500);
}

// --- buying and taxes -------------------------------------------------------

#[test]
fn an_unowned_street_offers_the_buy_prompt() {
    let mut g = game(2, 1500);
    place(&mut g, 0, MEDITERRANEAN - 1);
    g.apply_roll(0, 1);
    assert!(matches!(g.modal, Modal::Buy { pos, .. } if pos == MEDITERRANEAN));
}

#[test]
fn buying_debits_the_price_and_records_the_owner() {
    let mut g = game(2, 1500);
    place(&mut g, 0, MEDITERRANEAN);
    g.buy_current();
    assert_eq!(g.players[0].money, 1440);
    assert_eq!(g.board[MEDITERRANEAN].owner(), Some(0));
}

#[test]
fn buying_is_refused_without_the_cash() {
    let mut g = game(2, 1500);
    g.players[0].money = 10;
    place(&mut g, 0, MEDITERRANEAN);
    g.buy_current();
    assert_eq!(g.board[MEDITERRANEAN].owner(), None);
    assert_eq!(g.players[0].money, 10);
}

#[test]
fn buying_an_owned_space_is_refused() {
    let mut g = game(2, 1500);
    own(&mut g, MEDITERRANEAN, 1);
    place(&mut g, 0, MEDITERRANEAN);
    g.buy_current();
    assert_eq!(g.board[MEDITERRANEAN].owner(), Some(1));
    assert_eq!(g.players[0].money, 1500);
}

#[test]
fn tax_squares_bill_the_bank() {
    let mut g = game(2, 1500);
    place(&mut g, 0, INCOME_TAX - 2);
    g.apply_roll(1, 1);
    assert_eq!(g.players[0].money, 1300);

    let mut g = game(2, 1500);
    place(&mut g, 0, LUXURY_TAX - 3);
    g.apply_roll(1, 2);
    assert_eq!(g.players[0].money, 1400);
}

// --- turn order and elimination ---------------------------------------------

#[test]
fn end_turn_skips_eliminated_players() {
    let mut g = game(3, 1500);
    g.players[1].bankrupt = true;
    g.end_turn();
    assert_eq!(g.current, 2);
}

#[test]
fn bankruptcy_to_a_player_hands_over_the_estate() {
    let mut g = game(3, 1500);
    own(&mut g, MEDITERRANEAN, 0);
    set_houses(&mut g, MEDITERRANEAN, 2);
    g.bankrupt(0, Some(1));
    assert!(g.players[0].bankrupt);
    assert_eq!(g.board[MEDITERRANEAN].owner(), Some(1));
    assert_eq!(g.board[MEDITERRANEAN].houses(), 2, "buildings follow the deed");
}

#[test]
fn bankruptcy_to_the_bank_clears_the_estate() {
    let mut g = game(3, 1500);
    own(&mut g, MEDITERRANEAN, 0);
    set_houses(&mut g, MEDITERRANEAN, 2);
    g.board[MEDITERRANEAN].set_mortgaged(true);
    g.bankrupt(0, None);
    assert_eq!(g.board[MEDITERRANEAN].owner(), None);
    assert_eq!(g.board[MEDITERRANEAN].houses(), 0);
    assert!(!g.board[MEDITERRANEAN].is_mortgaged());
}

#[test]
fn a_bank_bankruptcy_auctions_the_estate_off() {
    let mut g = game(3, 1500);
    own(&mut g, MEDITERRANEAN, 0);
    own(&mut g, BOARDWALK, 0);
    g.bankrupt(0, None);

    assert!(matches!(g.modal, Modal::Auction(_)), "the first lot goes up at once");
    assert_eq!(g.pending.len(), 1, "the second waits its turn");
}

#[test]
fn each_lot_goes_up_as_the_last_one_closes() {
    let mut g = game(3, 1500);
    own(&mut g, MEDITERRANEAN, 0);
    own(&mut g, BOARDWALK, 0);
    g.bankrupt(0, None);

    g.handle_key(KeyCode::Char('b')); // player 2 bids on the first lot
    g.handle_key(KeyCode::Char('p')); // player 3 passes, ending it
    assert_eq!(g.board[MEDITERRANEAN].owner(), Some(1));
    assert!(matches!(g.modal, Modal::Auction(_)), "straight on to Boardwalk");

    g.handle_key(KeyCode::Char('b'));
    g.handle_key(KeyCode::Char('p'));
    assert_eq!(g.board[BOARDWALK].owner(), Some(1));
    assert!(matches!(g.modal, Modal::None), "and the queue is empty");
}

#[test]
fn a_creditor_owes_interest_on_inherited_mortgages() {
    let mut g = game(3, 1500);
    own(&mut g, BOARDWALK, 0);
    g.board[BOARDWALK].set_mortgaged(true);
    g.bankrupt(0, Some(1));

    assert_eq!(g.board[BOARDWALK].owner(), Some(1));
    assert!(g.board[BOARDWALK].is_mortgaged(), "it stays mortgaged");
    assert_eq!(g.players[1].money, 1480, "10% of the $200 mortgage value");
}

#[test]
fn an_unmortgaged_inheritance_costs_the_creditor_nothing() {
    let mut g = game(3, 1500);
    own(&mut g, BOARDWALK, 0);
    g.bankrupt(0, Some(1));
    assert_eq!(g.players[1].money, 1500);
}

#[test]
fn winning_the_game_cancels_any_queued_lots() {
    let mut g = game(2, 1500);
    own(&mut g, MEDITERRANEAN, 1);
    own(&mut g, BOARDWALK, 1);
    g.bankrupt(1, None);
    assert!(matches!(g.modal, Modal::GameOver(0)));
    assert!(g.pending.is_empty(), "no auctions once the game is over");
}

#[test]
fn a_bystander_going_bankrupt_does_not_end_the_current_turn() {
    let mut g = game(3, 1500);
    g.current = 0;
    g.bankrupt(2, Some(0));
    assert_eq!(g.current, 0, "player 1 keeps their turn");
}

#[test]
fn the_last_player_standing_wins() {
    let mut g = game(2, 1500);
    g.bankrupt(1, Some(0));
    assert!(matches!(g.modal, Modal::GameOver(0)));
}

#[test]
fn the_game_continues_while_two_players_remain() {
    let mut g = game(3, 1500);
    g.bankrupt(2, Some(0));
    assert!(!matches!(g.modal, Modal::GameOver(_)));
}

#[test]
fn active_players_omits_the_eliminated() {
    let mut g = game(3, 1500);
    g.players[1].bankrupt = true;
    assert_eq!(g.active_players(), vec![0, 2]);
}

#[test]
fn holdings_lists_only_your_own_spaces_in_board_order() {
    let mut g = game(2, 1500);
    own(&mut g, BOARDWALK, 0);
    own(&mut g, MEDITERRANEAN, 0);
    own(&mut g, BALTIC, 1);
    assert_eq!(g.holdings(0), vec![MEDITERRANEAN, BOARDWALK]);
}

// --- input dispatch ---------------------------------------------------------

#[test]
fn space_opens_the_roll_popup() {
    let mut g = game(2, 1500);
    g.handle_key(KeyCode::Char(' '));
    assert!(matches!(g.modal, Modal::Roll(_)));
}

#[test]
fn a_second_roll_is_refused_until_the_turn_ends() {
    let mut g = game(2, 1500);
    g.can_roll = false;
    g.handle_key(KeyCode::Char(' '));
    assert!(matches!(g.modal, Modal::None));
}

#[test]
fn ending_a_turn_needs_a_roll_first() {
    let mut g = game(2, 1500);
    g.run(TurnAction::EndTurn);
    assert!(matches!(g.modal, Modal::None), "no confirmation without a roll");

    g.has_rolled = true;
    g.run(TurnAction::EndTurn);
    assert!(matches!(g.modal, Modal::ConfirmEnd(_)));
}

#[test]
fn confirming_the_end_prompt_passes_the_turn() {
    let mut g = game(2, 1500);
    g.has_rolled = true;
    g.run(TurnAction::EndTurn);
    g.handle_key(KeyCode::Up); // move off the default "No"
    g.handle_key(KeyCode::Enter);
    assert_eq!(g.current, 1);
    assert!(matches!(g.modal, Modal::None));
}

#[test]
fn declining_the_end_prompt_keeps_the_turn() {
    let mut g = game(2, 1500);
    g.has_rolled = true;
    g.run(TurnAction::EndTurn);
    g.handle_key(KeyCode::Enter); // defaults to "No"
    assert_eq!(g.current, 0);
    assert!(matches!(g.modal, Modal::None));
}

#[test]
fn the_inventory_lists_what_you_own() {
    let mut g = game(2, 1500);
    own(&mut g, MEDITERRANEAN, 0);
    own(&mut g, READING_RR, 0);
    own(&mut g, BOARDWALK, 1);
    g.show_inventory();
    match &g.modal {
        Modal::Info(info) => assert_eq!(info.lines.len(), 2),
        _ => panic!("expected the inventory popup"),
    }
}

#[test]
fn the_game_over_popup_releases_the_screen() {
    let mut g = game(2, 1500);
    g.bankrupt(1, Some(0));
    assert!(!g.is_done());
    g.handle_key(KeyCode::Enter);
    assert!(g.is_done());
}

#[test]
fn the_tick_idles_when_nothing_is_animating() {
    let mut g = game(2, 1500);
    while g.needs_tick() {
        g.tick(Duration::from_millis(33));
    }
    assert!(!g.needs_tick());
    g.handle_key(KeyCode::Char(' '));
    assert!(g.needs_tick(), "a live roll must keep the loop polling");
}
