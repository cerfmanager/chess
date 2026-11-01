# Building a Bitboard Chess Engine

A guide to the concepts behind a modern chess engine, in the order you'd build them. Each section explains **what** the technique is, **why** it exists, and **how** it typically works. Code snippets are language-neutral pseudocode (close to C/Rust).

---

## Table of Contents

1. [Board Representation (Bitboards)](#1-board-representation-bitboards)
2. [Attack Tables](#2-attack-tables)
3. [Move Generation](#3-move-generation)
4. [Perft Testing](#4-perft-testing)
5. [The UCI Protocol](#5-the-uci-protocol)
6. [Basic Evaluation](#6-basic-evaluation)
7. [Basic Search](#7-basic-search)
8. [Transposition Table](#8-transposition-table)
9. [Search Enhancements](#9-search-enhancements)
10. [Better Evaluation](#10-better-evaluation)
11. [Measuring Strength](#11-measuring-strength)
12. [Common Pitfalls](#12-common-pitfalls)
13. [Resources](#13-resources)

---

## 1. Board Representation (Bitboards)

### What a bitboard is

A chessboard has 64 squares and a CPU register has 64 bits. A **bitboard** is a 64-bit unsigned integer where each bit says whether a square has some property, for example "there is a white knight here".

```
White pawns at the start:

8  . . . . . . . .
7  . . . . . . . .
6  . . . . . . . .
5  . . . . . . . .
4  . . . . . . . .
3  . . . . . . . .
2  1 1 1 1 1 1 1 1     = 0x000000000000FF00
1  . . . . . . . .
   a b c d e f g h
```

The power of this layout is **set-wise operations**: one CPU instruction acts on all 64 squares at once.

| Question | Bitboard expression |
|---|---|
| All white pieces | `wp \| wn \| wb \| wr \| wq \| wk` |
| Empty squares | `~all_occupied` |
| Push all white pawns one square | `(wp << 8) & empty` |
| Which of my pieces are attacked? | `my_pieces & enemy_attacks` |

### Layout options

- **12 bitboards**: one per (colour, piece type). Simple and direct.
- **6 + 2 bitboards**: one per piece type (both colours) plus one per colour. White knights are `knights & white`. Slightly more compact, and many engines prefer it.

Either way, also keep **occupancy bitboards** (white, black, all) up to date. You'll need them constantly. Many engines also keep a `mailbox[64]` array mapping square → piece, so "what's on e4?" is an O(1) lookup instead of checking 12 bitboards.

### Square mapping

You must choose which bit is which square. The most common convention is **Little-Endian Rank-File (LERF)**:

```
a1 = 0, b1 = 1, ..., h1 = 7
a2 = 8, ...
a8 = 56, ..., h8 = 63

square = rank * 8 + file
```

With this mapping, moving "north" (up one rank) is `<< 8`, "south" is `>> 8`, "east" is `<< 1`, and "west" is `>> 1`. Pick one mapping and never mix them; most subtle bugs come from inconsistency here.

### Other state

Bitboards don't capture everything. A position also needs:

- **Side to move**
- **Castling rights**: 4 bits (white kingside, white queenside, black kingside, black queenside)
- **En passant square**: the square *behind* a pawn that just double-pushed, or none
- **Halfmove clock**: plies since the last capture or pawn move (for the 50-move rule)
- **Fullmove number**
- **Zobrist hash** (see section 3)

### Essential bit helpers

```
popcount(bb)   // number of set bits  → counting pieces, mobility
lsb(bb)        // index of lowest set bit (trailing zero count)
pop_lsb(&bb)   // return lsb and clear it:  sq = lsb(bb); bb &= bb - 1;
```

`bb & (bb - 1)` clears the lowest set bit. This gives you the standard loop for iterating over pieces:

```
while bb != 0 {
    sq = pop_lsb(&bb)
    // do something with sq
}
```

Use your language's intrinsics (`trailing_zeros()` / `count_ones()` in Rust, `__builtin_ctzll` / `__builtin_popcountll` in C). They compile to single instructions.

### FEN and printing

**FEN** (Forsyth–Edwards Notation) is the standard text format for a position:

```
rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1
   piece placement (rank 8 → 1)           side castling ep half full
```

Write a FEN parser and a board printer before anything else. You will use them in every test and every debugging session.

---

## 2. Attack Tables

Move generation keeps asking "which squares does a piece on square X attack?". For most pieces the answer never changes, so you compute it once at startup.

### Leapers: knights, kings, pawns

These pieces' attacks depend only on their square, not on other pieces. Build arrays like `KNIGHT_ATTACKS[64]` by shifting a single bit in each direction.

**The wraparound problem**: shifting a bit on the h-file east by 1 puts it on the a-file of the next rank. Fix this with file masks:

```
NOT_A_FILE  = 0xFEFEFEFEFEFEFEFE
NOT_H_FILE  = 0x7F7F7F7F7F7F7F7F
NOT_AB_FILE = 0xFCFCFCFCFCFCFCFC
NOT_GH_FILE = 0x3F3F3F3F3F3F3F3F

// knight moving 2 up, 1 right:
(b << 17) & NOT_A_FILE    // anything landing on the a-file wrapped around
```

Pawn attacks need a table per colour: `PAWN_ATTACKS[colour][64]`.

### Sliders: rooks, bishops, queens

Slider attacks depend on **blockers**. A rook on a1 attacks the whole a-file only if nothing is in the way. A queen is just rook attacks OR bishop attacks.

**Classical / ray approach (start here).** Precompute a ray in each of the 8 directions from every square. To get attacks in one direction:

1. Take the ray.
2. Find the first blocker on it (`ray & occupied`), using lsb for positive directions or msb for negative ones.
3. Remove everything beyond that blocker by XORing with the blocker's own ray in the same direction.

This is easy to get right and fast enough to start with.

**Magic bitboards (the standard fast method).** Key observation: for a rook on a given square, only the blockers on its rays matter, and not even the edge squares (a piece on the edge doesn't block anything behind it). So:

1. **Relevant mask**: the rook's rays from that square, excluding edge squares. At most 12 bits for a rook and 9 for a bishop.
2. Every subset of that mask is a possible blocker configuration (up to 2¹² = 4096 for a rook).
3. Find a **magic number** so that multiplying the masked occupancy by it maps every configuration to a unique index (or to an index whose attack set happens to be identical):

```
index = ((occupied & MASK[sq]) * MAGIC[sq]) >> (64 - BITS[sq])
attacks = ROOK_TABLE[sq][index]
```

The multiplication "gathers" the scattered relevant bits into the top bits of the product. Magics are found by trial: generate sparse random 64-bit numbers and test them until one causes no destructive collisions. You can also copy known magics from the Chess Programming Wiki. Total table size is roughly 800 KB for rooks and 40 KB for bishops.

**PEXT bitboards.** On CPUs with BMI2, the `pext` instruction extracts the bits of `occupied` selected by `MASK[sq]` into a dense index directly, with no magic number needed. It's very fast on Intel and AMD Zen 3+, but slow on Zen 1/2, so many engines support both.

Get the classical version working and perft-correct first, then swap in magics. Perft will instantly tell you if the replacement is wrong.

---

## 3. Move Generation

### Move encoding

Moves are generated by the millions, so keep them small. A common 16-bit encoding:

```
bits  0-5   from square
bits  6-11  to square
bits 12-15  flags
```

Typical flag values: quiet move, double pawn push, king castle, queen castle, capture, en passant capture, and promotions (knight/bishop/rook/queen, each with and without capture). With well-chosen flags you can test "is capture" or "is promotion" with a single bit check.

### Pseudo-legal vs. legal

- **Pseudo-legal** moves follow piece movement rules but may leave your own king in check (e.g. moving a pinned piece).
- **Legal** moves are the actual legal set.

**Simple approach**: generate pseudo-legal moves, make each one, check whether your king is attacked, and unmake it if so. This is easy and correct. You can also defer the check: only test legality when the search actually tries the move.

**Faster approach**: compute **checkers** (pieces giving check) and **pin masks** first, then generate only legal moves:

- In double check, only king moves are legal.
- In single check, other pieces may only capture the checker or block the line between it and the king.
- Pinned pieces may only move along their pin ray.
- King moves must avoid squares the enemy attacks (computed with your own king removed from the occupancy, so you can't "hide" behind yourself on a slider's ray).

### Is a square attacked?

Use the attack tables in reverse. A square is attacked by black if a black knight sits on a square a knight on the target square could reach, and the same idea for the other pieces:

```
is_attacked(sq, by_black) =
    (KNIGHT_ATTACKS[sq]          & black_knights)              != 0
 || (KING_ATTACKS[sq]            & black_king)                 != 0
 || (PAWN_ATTACKS[WHITE][sq]     & black_pawns)                != 0   // note: reversed colour
 || (bishop_attacks(sq, occ)     & (black_bishops|black_queens)) != 0
 || (rook_attacks(sq, occ)       & (black_rooks|black_queens))   != 0
```

### Make and unmake

**Make** applies a move: move the piece bits, remove captured pieces, handle promotion, castling (move the rook too), and en passant (remove the pawn *behind* the target square), then update castling rights, the en passant square, clocks, side to move, and the hash.

Two strategies:

- **Make/unmake**: push the irreversible state (captured piece, castling rights, ep square, halfmove clock, hash) onto an undo stack and reverse the move afterwards.
- **Copy-make**: copy the whole position struct, make the move on the copy, and discard it afterwards. The position is ~100–150 bytes, so this is often just as fast and much harder to get wrong.

**Updating castling rights** elegantly: keep a 64-entry table of masks. Whenever a move touches a square (from *or* to), AND the rights with that square's mask. Touching e1 clears both white rights; touching h1 clears white kingside, and so on. This automatically handles rooks being captured on their home squares.

### Zobrist hashing

You need a fast, nearly unique 64-bit fingerprint for each position (for repetition detection and the transposition table).

1. At startup, generate random 64-bit numbers:
   - one per (piece, square): 12 × 64
   - one for "black to move"
   - one per castling-rights combination (16) or per right (4)
   - one per en passant file (8)
2. The hash of a position is the XOR of the numbers for everything present.
3. Because XOR is its own inverse, you can **update incrementally**: moving a knight from g1 to f3 is `hash ^= Z[WN][g1] ^ Z[WN][f3]`. Flip side to move with `hash ^= Z_SIDE`.

Write a `compute_hash_from_scratch()` function and, in debug builds, assert that it matches the incremental hash after every make/unmake. This catches many bugs.

---

## 4. Perft Testing

**Perft** ("performance test") counts every leaf node of the legal move tree to a fixed depth:

```
perft(pos, depth):
    if depth == 0: return 1
    total = 0
    for move in legal_moves(pos):
        make(move)
        total += perft(pos, depth - 1)
        unmake(move)
    return total
```

The correct counts for many positions are known exactly, so any mismatch proves a bug in move generation, make/unmake, or attack detection. **A chess engine built on buggy movegen will never be reliable**, and search bugs are much harder to find than movegen bugs.

### Reference numbers

Starting position:

| Depth | Nodes |
|---|---|
| 1 | 20 |
| 2 | 400 |
| 3 | 8,902 |
| 4 | 197,281 |
| 5 | 4,865,609 |
| 6 | 119,060,324 |

**Kiwipete** (`r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -`) is packed with castling, en passant, pins, and promotions:

| Depth | Nodes |
|---|---|
| 1 | 48 |
| 2 | 2,039 |
| 3 | 97,862 |
| 4 | 4,085,603 |

The Chess Programming Wiki's *Perft Results* page lists more positions; run them all.

### Divide

When a count is wrong, **divide** prints perft(depth − 1) for each root move separately. Run the same position in Stockfish (`position fen ...` then `go perft N`), compare line by line, find the move whose count differs, play it, and repeat at lower depth. Within a few iterations you'll be looking at the exact position and move your generator gets wrong.

Perft also works as a **speed benchmark** for movegen (nodes per second).

---

## 5. The UCI Protocol

The **Universal Chess Interface** is a text protocol over stdin/stdout. Supporting it means your engine works with any GUI (Cute Chess, Arena, En Croissant) and with automated testing tools.

A minimal session:

```
GUI → uci
ENG ← id name MyEngine
ENG ← id author Alex
ENG ← uciok
GUI → isready
ENG ← readyok
GUI → ucinewgame
GUI → position startpos moves e2e4 e7e5
GUI → go wtime 60000 btime 60000 winc 1000 binc 1000
ENG ← info depth 8 score cp 25 nodes 120345 pv g1f3 b8c6 ...
ENG ← bestmove g1f3
GUI → quit
```

Things to know:

- Moves use **long algebraic** notation: `e2e4`, `e7e8q` for promotion, `e1g1` for castling.
- `go` can carry `depth`, `movetime`, `nodes`, `infinite`, or clock times (`wtime`, `btime`, `winc`, `binc`, `movestogo`).
- `stop` must interrupt the search, so the search runs on a separate thread (or polls stdin) and checks a stop flag every few thousand nodes.
- Always reply with `bestmove`, even when stopped early.

A simple **time management** rule to start: spend about `time_left / 30 + increment / 2` per move, and stop iterative deepening when the next iteration probably won't finish.

---

## 6. Basic Evaluation

The evaluation function estimates how good a quiet position is, in **centipawns** (1 pawn = 100).

### Material

```
Pawn 100, Knight 320, Bishop 330, Rook 500, Queen 900
```

Material alone already produces a weak but playable engine.

### Piece-square tables (PSTs)

A PST is a 64-entry array per piece type giving a bonus or penalty depending on where the piece stands. Knights get bonuses in the centre and penalties on the rim; pawns gain value as they advance; the king prefers to stay castled in the middlegame. PSTs are cheap, and you can update them incrementally in make/unmake.

For black, mirror the square vertically (`sq ^ 56` in LERF).

### Side-to-move perspective

Compute `white_score - black_score`, then **negate it if black is to move**. Negamax (below) requires the score always to be "good for the side to move".

---

## 7. Basic Search

### Minimax and negamax

**Minimax** explores the game tree: you pick the move that maximises your score, assuming the opponent picks the move that minimises it. **Negamax** is the same idea using the identity `max(a, b) = -min(-a, -b)`, so a single function serves both sides:

```
negamax(pos, depth):
    if depth == 0: return evaluate(pos)
    best = -INF
    for move in moves(pos):
        make(move)
        score = -negamax(pos, depth - 1)
        unmake(move)
        best = max(best, score)
    return best
```

### Alpha-beta pruning

Most of the tree is irrelevant. Alpha-beta tracks a window:

- **alpha**: the best score the side to move is already guaranteed elsewhere
- **beta**: the best score the opponent is already guaranteed elsewhere; if we exceed it, they will avoid this line

```
alphabeta(pos, depth, alpha, beta):
    if depth == 0: return quiescence(pos, alpha, beta)
    for move in ordered_moves(pos):
        make(move)
        score = -alphabeta(pos, depth - 1, -beta, -alpha)
        unmake(move)
        if score >= beta: return beta        // fail-high: "beta cutoff"
        if score > alpha: alpha = score      // new best move
    return alpha
```

When a move refutes the opponent's choice (score ≥ beta), the remaining moves don't need to be searched. With **perfect move ordering**, alpha-beta searches roughly the square root of the minimax tree, which means about twice the depth in the same time. That is why move ordering matters so much.

### Iterative deepening

Search depth 1, then depth 2, then 3, and so on, until time runs out. This looks wasteful but isn't:

- The tree grows exponentially, so earlier iterations cost a small fraction of the last one.
- You always have a complete result to return when time runs out.
- Each iteration's best move (and TT entries) improve ordering in the next one, so the total is often *faster* than one direct deep search.

### Quiescence search

If you stop at a fixed depth and evaluate, you may stop in the middle of an exchange: "I just took a knight with my queen!", while the pawn recapture is one ply beyond the horizon. This is the **horizon effect**.

**Quiescence search** continues searching only **captures** (and possibly promotions) at the leaves until the position is quiet:

```
quiescence(pos, alpha, beta):
    stand_pat = evaluate(pos)          // "I could just not capture"
    if stand_pat >= beta: return beta
    alpha = max(alpha, stand_pat)
    for capture in ordered_captures(pos):
        make(capture)
        score = -quiescence(pos, -beta, -alpha)
        unmake(capture)
        if score >= beta: return beta
        alpha = max(alpha, score)
    return alpha
```

The **stand-pat** score assumes that the side to move usually has at least one quiet move that keeps the current evaluation.

### Mate, stalemate, and draws

- If there are no legal moves: return `-MATE + ply` when in check (checkmate), otherwise `0` (stalemate). Including `ply` makes the engine prefer **faster mates** and delay being mated.
- **Repetition**: keep a stack of hashes for the game and the search path. If the current hash appeared before (only positions since the last irreversible move can match, and only with the same side to move), return a draw score. Most engines treat a single repetition inside the search as a draw.
- **50-move rule**: if the halfmove clock reaches 100, it's a draw.

### Move ordering: MVV-LVA

**Most Valuable Victim – Least Valuable Attacker**: search captures first, sorted by the value of the captured piece, breaking ties by the cheapest attacker. PxQ comes first and QxP comes late. It's simple, cheap, and dramatically increases the number of beta cutoffs.

---

## 8. Transposition Table

Different move orders often reach the same position (**transpositions**). A **transposition table (TT)** is a large hash table indexed by the Zobrist key that caches search results.

Each entry stores:

| Field | Purpose |
|---|---|
| Key (or its upper bits) | Verify the entry belongs to this position |
| Depth | How deep the stored search was |
| Score | The result |
| Bound type | `EXACT`, `LOWER` (fail-high, true score ≥ stored), or `UPPER` (fail-low, true score ≤ stored) |
| Best move | The move to try first next time |
| Age (optional) | For replacement decisions across searches |

How it's used:

```
entry = tt.probe(hash)
if entry and entry.depth >= depth:
    if entry.bound == EXACT:                         return entry.score
    if entry.bound == LOWER and entry.score >= beta:  return entry.score
    if entry.bound == UPPER and entry.score <= alpha: return entry.score
// otherwise, at least search entry.best_move first
```

Even when the stored depth is too shallow to return a score, the **TT move is usually the best move ordering signal you have**.

**Indexing**: with a power-of-two table size, `index = hash & (size - 1)`.

**Mate scores**: mate scores are relative to the root (`MATE - ply`), but TT entries are reused at different plies. Convert to "distance from this node" when storing (`score + ply` for positive mates, `score - ply` for negative) and convert back when probing. Getting this wrong produces engines that announce bogus mates.

---

## 9. Search Enhancements

Add these **one at a time**, and test each one (section 11). Some interact in surprising ways.

### Killer moves

A quiet move that caused a beta cutoff at some ply is likely to cause one in sibling positions at the same ply. Store two "killer" moves per ply and try them right after captures.

### History heuristic

Keep a table `history[side][from][to]` (or `[piece][to]`). When a quiet move causes a cutoff, add a bonus (often `depth²`); optionally penalise quiet moves that were tried and failed. Sort the remaining quiet moves by this score.

A typical full ordering: **TT move → winning captures (MVV-LVA/SEE) → killers → quiets by history → losing captures**.

### Principal Variation Search (PVS)

With good ordering, the first move is usually best. So search the first move with the full window, and every other move with a **null window** `(alpha, alpha + 1)`, which only answers "is this better than alpha, yes or no?" and is much cheaper. If a move surprisingly returns a score above alpha, re-search it with the full window.

```
if first_move:
    score = -search(depth-1, -beta, -alpha)
else:
    score = -search(depth-1, -alpha-1, -alpha)       // zero-window probe
    if alpha < score < beta:
        score = -search(depth-1, -beta, -alpha)      // re-search
```

### Null-move pruning

Let the side to move "pass" (make a null move) and search with reduced depth (typically `depth - 1 - R`, with R ≈ 2–3). If the opponent still can't bring the score below beta even with two moves in a row, the real position is almost certainly ≥ beta as well, so prune.

Don't use it when in check, and be careful in pawn-only or low-material endgames, where **zugzwang** (any move worsens your position) makes the "passing is never better than moving" assumption false.

### Late Move Reductions (LMR)

Moves late in a well-ordered list rarely turn out to be best. Search them at reduced depth (e.g. reduction based on `log(depth) * log(move_number)`) with a null window. If one unexpectedly beats alpha, re-search it at full depth. Usually you exempt captures, promotions, killers, checks, and moves made while in check.

LMR is one of the largest single strength gains in modern engines.

### Aspiration windows

At the root, instead of searching with `(-INF, +INF)`, search with a narrow window around the previous iteration's score, e.g. `(prev - 25, prev + 25)`. Narrow windows cause more cutoffs. If the result falls outside the window, widen it and re-search.

### Check extensions

When the side to move is in check, search one ply deeper. Checks are forcing and often lead to tactics just past the horizon, and in-check positions have few legal moves, so the cost is small.

### Futility pruning and delta pruning

- **Futility pruning**: near the leaves (depth 1–2), if `static_eval + margin` (e.g. a few hundred centipawns) is still below alpha, a quiet move is very unlikely to raise the score enough, so skip quiet moves.
- **Delta pruning**: the quiescence version. If `stand_pat + value(captured_piece) + margin < alpha`, even winning that piece can't help, so skip the capture.

### Static Exchange Evaluation (SEE)

SEE simulates the full sequence of captures on one square, always recapturing with the least valuable attacker, and returns the net material result. For example, NxP defended by a pawn is −320 + 100 = a loss. Uses:

- Order **winning** captures before quiets and **losing** captures after them.
- Prune losing captures in quiescence search.

Implementing SEE requires recomputing slider attacks as pieces are removed from the square's lines, to reveal x-ray attackers behind them.

---

## 10. Better Evaluation

### Tapered evaluation

Piece values and good squares change through the game (the king should hide in the middlegame but centralise in the endgame). Keep two scores, **midgame (mg)** and **endgame (eg)**, and blend them by a **game phase** computed from remaining non-pawn material:

```
phase = (knights + bishops) * 1 + rooks * 2 + queens * 4    // 24 at the start
score = (mg * phase + eg * (24 - phase)) / 24
```

### Common evaluation terms

- **Mobility**: number of squares each piece attacks (excluding squares controlled by enemy pawns or own pieces). Cheap with bitboards: `popcount(attacks & ~own & ~enemy_pawn_attacks)`.
- **Pawn structure**:
  - *Doubled* pawns: two own pawns on one file. Weak.
  - *Isolated* pawns: no own pawns on adjacent files. Weak.
  - *Passed* pawns: no enemy pawns ahead on the same or adjacent files. Strong, more so as they advance and in the endgame.
  - Precompute masks (e.g. `PASSED_MASK[colour][sq]`) so each test is one AND.
  - Pawn structure changes rarely, so cache it in a small **pawn hash table** keyed by a pawn-only Zobrist hash.
- **King safety**: pawn shield in front of the king, open files near it, and the number and weight of enemy pieces attacking the king zone. This is very important in middlegames.
- **Bishop pair**: a bonus (~30–50 cp) for having both bishops.
- **Rooks on open/semi-open files** and on the 7th rank.

### Tuning

Hand-picked numbers are only guesses. **Texel tuning** finds better values automatically:

1. Collect a large set of quiet positions labelled with game results (1, 0.5, 0).
2. Map your eval to a predicted win probability with a sigmoid: `1 / (1 + 10^(-K * eval / 400))`.
3. Adjust parameters to minimise the mean squared error between predictions and results, using local search or (better) gradient descent.

### NNUE

**Efficiently Updatable Neural Networks** replace the hand-written eval with a small neural network. The first layer's input is piece-square features, and because a move changes only a few features, the first layer's output (the "accumulator") can be updated incrementally in make/unmake rather than recomputed. NNUE is what modern top engines use. It's a great later project once you have a good engine to generate training data with.

---

## 11. Measuring Strength

Chess engine development is mostly **testing**. Many plausible-sounding changes make engines weaker, and the effect sizes are small (a few Elo), so you need many games to tell.

### Self-play matches

Use **cutechess-cli** or **fastchess** to play your new version against the previous one:

- Short time controls (e.g. 10s + 0.1s per game) so you can play thousands of games.
- An **opening book** of varied, balanced positions, with each opening played twice with colours swapped to cancel out opening bias.

### SPRT

The **Sequential Probability Ratio Test** decides whether a change is an improvement with controlled error rates. You choose two hypotheses (e.g. "gain ≤ 0 Elo" vs "gain ≥ 5 Elo"), and the test plays games until the evidence crosses one of the bounds. It stops early on clear results and keeps going on close ones. fastchess and cutechess-cli support it directly. This is how serious engines (Stockfish included) accept or reject every change.

### Absolute strength

To estimate a rating, play gauntlets against engines with known ratings (e.g. from the CCRL lists), or against Stockfish with `UCI_LimitStrength` and `UCI_Elo` set.

### Version control

Commit every change and tag every tested version. When something regresses, `git bisect` with a quick match or perft/bench check will find it. Many engines also have a `bench` command that searches a fixed set of positions to a fixed depth and prints the node count. If that count changes, the search behaviour changed; it's a cheap fingerprint for "did this refactor change anything?".

---

## 12. Common Pitfalls

- **Shift wraparound.** Shifting east from the h-file or west from the a-file wraps onto the next rank. Always mask.
- **En passant exposing the king horizontally.** After `exd6 e.p.`, *two* pawns leave the 5th rank. If your king and an enemy rook or queen share that rank, the capture is illegal. Normal pin detection misses this because it looks for one piece between king and attacker.
- **Castling through check.** The king may not castle out of check, through an attacked square, or into check. (The rook may pass through an attacked square; on the queenside, b1/b8 must be empty but may be attacked.)
- **Castling rights when a rook is captured.** If your h1 rook is captured on h1, you lose kingside castling. The square-mask table from section 3 handles this automatically.
- **En passant square in the hash.** Only hash it when an en passant capture is actually possible; otherwise identical positions get different hashes, and repetitions go undetected.
- **Mate scores in the TT.** Adjust by ply when storing and probing (section 8).
- **Negation errors.** Forgetting to negate the score or swap and negate the window in negamax is a classic bug. Perft won't catch it, but games against yourself will look absurd.
- **Promotion moves in UCI.** Parse and emit the promotion letter (`e7e8q`), and don't forget underpromotions.
- **Not checking the stop flag.** The engine will lose on time.

---

## 13. Resources

- **Chess Programming Wiki** (chessprogramming.org): the reference for every topic here.
- **Perft Results** page on the wiki: standard test positions and node counts.
- **Readable engines** worth studying *after* writing your own version of each part:
  - *TSCP*: tiny and classic (not bitboard-based, but shows search clearly)
  - *VICE*: accompanied by a long video series
  - *Blunder* (Go), *Rustic* (Rust, with a written book), and *Weiss* (C) are clean bitboard engines
- **Tools**: Cute Chess (GUI), cutechess-cli / fastchess (testing), Stockfish (reference perft and sparring partner).

---

### Suggested build order recap

1. Bitboards, FEN, attack tables, movegen, make/unmake, Zobrist → **perft passes**
2. UCI, material + PST eval, alpha-beta, iterative deepening, quiescence, MVV-LVA → **plays real games**
3. Transposition table → **big jump in depth**
4. Killers, history, PVS, null move, LMR, aspiration windows, extensions, pruning, SEE → **tested one by one with SPRT**
5. Tapered eval, pawn structure, king safety, tuning, then NNUE → **long-term strength**
