# Monopoly — Game Rules

A plain-text reference for the rules this project implements. Based on the
standard US edition. Sources are listed at the bottom.

## Setup

- 2 to 8 players.
- Each player starts with $1500 (this game lets you adjust in $250 steps).
- Every player starts on GO.
- One player acts as the Bank (in this game, the Bank is automatic).

## Turn Order

- To decide who goes first, each player rolls the dice; highest total starts.
- Play then passes to the next player (clockwise / to the left).

## A Turn

1. Roll both dice.
2. Move your token that many spaces, in board order, around the board.
3. Resolve the space you land on (see below).
4. Optionally buy/sell/trade/build/mortgage.
5. End your turn (unless you rolled doubles).

### Doubles

- Roll doubles: take your turn, then roll and move again.
- Roll doubles three times in one turn: go directly to Jail (do not pass GO).

## Passing GO

- Each time you land on or pass over GO, the Bank pays you a $200 salary.
- Paid once per pass.

## Spaces

### Property (streets)

- If unowned: you may buy it from the Bank at its printed price.
- If you decline: the Bank auctions it to the highest bidder (any player).
- If owned by another player: pay them rent (rent rises with houses/hotels and
  with owning a full color group).

### Railroads

- Printed price $200.
- Rent depends on how many railroads the owner holds (more owned = higher rent).

### Utilities (Electric Company, Water Works)

- Printed price $150.
- Rent = dice roll x4 if the owner has one utility, x10 if they own both.

### Taxes

- Income Tax: pay $200.
- Luxury Tax: pay $100.

### Chance / Community Chest

- Draw the top card and follow its instructions.

### Corners

- GO: collect $200 when you pass or land.
- Jail / Just Visiting: if just visiting, nothing happens.
- Free Parking: nothing happens (officially no payout).
- Go To Jail: go directly to Jail; do not pass GO, do not collect $200.

## Jail

You are sent to Jail by landing on "Go To Jail", drawing a card, or rolling
three doubles in a row.

Three ways out:

1. Pay $50 before your roll.
2. Use a "Get Out of Jail Free" card.
3. Roll doubles (you get up to 3 attempts on consecutive turns).

After the third failed turn you must pay the $50 and move out.

## Building

- You must own every street in a color group to build on it.
- Build and sell evenly: no street may be more than one house ahead of its group.
- A hotel is the fifth house.
- The Bank holds 32 houses and 12 hotels; when it runs out, nobody can build.

## Mortgaging

- Mortgage an unbuilt property to the Bank for half its printed price.
- A mortgaged property collects no rent.
- To unmortgage: pay the mortgage value plus 10% interest.

## Bankruptcy

- If you owe more than you can pay (even after mortgaging/selling), you are
  bankrupt and out of the game.
- Bankrupt to a player: they take your cash and deeds, and owe 10% interest on
  each mortgaged one.
- Bankrupt to the Bank: your properties are auctioned off.
- Last player remaining wins.

## Sources

- Monopoly Wiki — Official Rules: https://monopoly.fandom.com/wiki/Official_Rules
- Hasbro Monopoly Rules (PDF): https://fgbradleys.com/wp-content/uploads/rules/Monopoly_Rules.pdf
- Monopoly Rules guide: https://hobbyscoop.com/monopoly-rules/
