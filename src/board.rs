const EMPTY: u64 = 0;
// Little-endian rank-file mapping: bit 0 = a1, bit 7 = h1, bit 63 = h8 (matches MAILBOX indexing).
const WHITE_PAWNS: u64 = 0x0000_0000_0000_FF00;
const WHITE_KNIGHTS: u64 = 0x0000_0000_0000_0042;
const WHITE_BISHOPS: u64 = 0x0000_0000_0000_0024;
const WHITE_ROOKS: u64 = 0x0000_0000_0000_0081;
const WHITE_QUEEN: u64 = 0x0000_0000_0000_0008;
const WHITE_KING: u64 = 0x0000_0000_0000_0010;

const BLACK_PAWNS: u64 = WHITE_PAWNS << 40;
const BLACK_KNIGHTS: u64 = WHITE_KNIGHTS << 56;
const BLACK_BISHOPS: u64 = WHITE_BISHOPS << 56;
const BLACK_ROOKS: u64 = WHITE_ROOKS << 56;
const BLACK_QUEEN: u64 = WHITE_QUEEN << 56;
const BLACK_KING: u64 = WHITE_KING << 56;

const NOT_A_FILE: u64 = 0xFEFEFEFEFEFEFEFE;
const NOT_H_FILE: u64 = 0x7F7F7F7F7F7F7F7F;
const NOT_AB_FILE: u64 = 0xFCFCFCFCFCFCFCFC;
const NOT_GH_FILE: u64 = 0x3F3F3F3F3F3F3F3F;

const MAILBOX: [(char, char); 64] = [
    ('w', 'r'),
    ('w', 'n'),
    ('w', 'b'),
    ('w', 'q'),
    ('w', 'k'),
    ('w', 'b'),
    ('w', 'n'),
    ('w', 'r'),
    ('w', 'p'),
    ('w', 'p'),
    ('w', 'p'),
    ('w', 'p'),
    ('w', 'p'),
    ('w', 'p'),
    ('w', 'p'),
    ('w', 'p'),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    (' ', ' '),
    ('b', 'p'),
    ('b', 'p'),
    ('b', 'p'),
    ('b', 'p'),
    ('b', 'p'),
    ('b', 'p'),
    ('b', 'p'),
    ('b', 'p'),
    ('b', 'r'),
    ('b', 'n'),
    ('b', 'b'),
    ('b', 'q'),
    ('b', 'k'),
    ('b', 'b'),
    ('b', 'n'),
    ('b', 'r'),
];

enum Color {
    Black,
    White,
}

struct CastleRights {
    white_kingside: bool,
    white_queenside: bool,
    black_kingside: bool,
    black_queenside: bool,
}

struct Board {
    pawn: u64,
    bishop: u64,
    knight: u64,
    rook: u64,
    queen: u64,
    king: u64,
    en_passant: u64,
}
// WHITE_ROOKS | WHITE_KING | WHITE_BISHOPS |WHITE_PAWNS |WHITE_QUEEN |WHITE_KNIGHTS |BLACK_BISHOPS|BLACK_KING|BLACK_KNIGHTS|BLACK_PAWN|BLACK_PAWN|BLACK_QUEEN|BLACK_ROOKS;
//  WHITE_ROOKS | WHITE_KING | WHITE_BISHOPS |WHITE_PAWNS |WHITE_QUEEN |WHITE_KNIGHTS
// BLACK_BISHOPS|BLACK_KING|BLACK_KNIGHTS|BLACK_PAWN|BLACK_PAWN|BLACK_QUEEN|BLACK_ROOKS
pub struct BoardState {
    white: Board,
    black: Board,
    occupancy_board: u64,
    mailbox: [(char, char); 64],
    next_player: Color,
    curr_eval: i64,
    castle_rights: CastleRights,
    move_number: u32,
    halfmove_clock: u32,
}

pub struct Attacks {
    knight_attacks: [u64; 64],
    white_pawn_attacks: [u64; 64],
    black_pawn_attacks: [u64; 64],
    king_attacks: [u64; 64],
}

impl BoardState {
    fn get_pawns(&mut self, color: Color) -> u64 {
        match color {
            Color::Black => self.black.pawn,
            Color::White => self.white.pawn,
        }
    }
}

pub fn main() {
    let board_state: BoardState = BoardState {
        white: Board {
            pawn: WHITE_PAWNS,
            bishop: WHITE_BISHOPS,
            knight: WHITE_KNIGHTS,
            rook: WHITE_ROOKS,
            queen: WHITE_QUEEN,
            king: WHITE_KING,
            en_passant: EMPTY,
        },
        black: Board {
            pawn: BLACK_PAWNS,
            bishop: BLACK_BISHOPS,
            knight: BLACK_KNIGHTS,
            rook: BLACK_ROOKS,
            queen: BLACK_QUEEN,
            king: BLACK_KING,
            en_passant: EMPTY,
        },
        occupancy_board: WHITE_ROOKS
            | WHITE_KING
            | WHITE_BISHOPS
            | WHITE_PAWNS
            | WHITE_QUEEN
            | WHITE_KNIGHTS
            | BLACK_BISHOPS
            | BLACK_KING
            | BLACK_KNIGHTS
            | BLACK_PAWNS
            | BLACK_QUEEN
            | BLACK_ROOKS,
        mailbox: MAILBOX,
        next_player: Color::White,
        curr_eval: 0,
        castle_rights: CastleRights {
            white_kingside: true,
            white_queenside: true,
            black_kingside: true,
            black_queenside: true,
        },
        move_number: 1,
        halfmove_clock: 0,
    };

    let leaper_attacks = Attacks {
        knight_attacks : build_knights_table(),
        king_attacks: build_king_table(),
        white_pawn_attacks:build_white_pawn(),
        black_pawn_attacks:build_black_pawn(),
    }

    println!("{}", board_state.white.pawn.count_ones());
    println!("{}", fen_parse(&board_state));
    board_printer(&board_state);
}

fn lsb(bitboard: u64) -> u64 {
    bitboard & bitboard - 1
}

fn pop_lsb(bitboard: &mut u64) -> u64 {
    let lsb = lsb(*bitboard);
    *bitboard &= lsb - 1;
    lsb
}

fn iterate_bitboard(mut bitboard: u64) {
    while bitboard != 0 {
        let lsb = pop_lsb(&mut bitboard);
        let index = lsb.trailing_zeros();
        println!("Bit index: {}", index);
    }
}

fn all_pieces(board: &Board) -> u64 {
    board.pawn | board.bishop | board.knight | board.rook | board.queen | board.king
}

pub fn fen_parse(board_state: &BoardState) -> String {
    let mut fen_string = String::new();
    for rank in 0..8 {
        let mut empty_count = 0;
        for file in 0..8 {
            let index = rank * 8 + file;
            let (color, piece) = board_state.mailbox[index as usize];
            if color == ' ' {
                empty_count += 1;
            } else {
                if empty_count > 0 {
                    fen_string.push_str(&empty_count.to_string());
                    empty_count = 0;
                }
                match color {
                    'b' => {
                        fen_string.push(piece.to_ascii_uppercase());
                    }
                    _ => {
                        fen_string.push(piece);
                    }
                }
            }
        }
        if empty_count > 0 {
            fen_string.push_str(&empty_count.to_string());
        }
        if rank != 7 {
            fen_string.push('/');
        }
    }
    fen_string
}
fn board_printer(board: &BoardState) {
    for (i, (color, piece)) in board.mailbox.iter().enumerate() {
        if i % 8 == 0 && i != 0 {
            println!();
        }
        match color {
            'w' => {
                print!("{}", piece)
            }
            _ => {
                print!("{}", piece.to_ascii_uppercase())
            }
        }
    }
}

fn knight_attacks(knights: u64) -> u64 {
    let l1 = (knights >> 1) & NOT_H_FILE;
    let l2 = (knights >> 2) & NOT_GH_FILE;
    let r1 = (knights << 1) & NOT_A_FILE;
    let r2 = (knights << 2) & NOT_AB_FILE;
    let h1 = l1 | r1;
    let h2 = l2 | r2;
    (h1 << 16) | (h1 >> 16) | (h2 << 8) | (h2 >> 8)
}

fn pawn_attacks_white(pawns: u64) -> u64 {
    ((pawns << 9) & NOT_A_FILE) | ((pawns << 7) & NOT_H_FILE)
}
fn pawn_attacks_black(pawns: u64) -> u64 {
    ((pawns >> 7) & NOT_A_FILE) | ((pawns >> 9) & NOT_H_FILE)
}


fn king_attacks(king: u64) -> u64 {
    let l = (king >> 1) & NOT_H_FILE;
    let r = (king << 1) & NOT_A_FILE;
    let row = king | l | r;
    (row << 8) | (row >> 8) | l | r
}

fn rook_attacks(rook:u64)->u64{
    let mut attacked = 0u64;
    for moved_sq in 1..8{
        attacked = attacked | rook << (moved_sq * 8);
        attacked = attacked | rook >> (moved_sq * 8);
        attacked = attacked | (rook << (moved_sq)) & NOT_A_FILE;
        attacked = attacked | (rook << (moved_sq)) & NOT_H_FILE;
    }

    attacked
}

fn bishop_attacks(bishop:u64)->u64{
    let mut attacked = 0u64;
    for moved_sq in 1..8{
        attacked = attacked |( bishop << 9 * moved_sq )& NOT_A_FILE ;
        attacked = attacked |( bishop << 7 * moved_sq) & NOT_H_FILE;
        attacked = attacked | (bishop >> 7*moved_sq)  & NOT_A_FILE;
        attacked = attacked | (bishop >>9* moved_sq) & NOT_H_FILE;
    }

    attacked
}

fn build_knights_table() -> [u64; 64] {
    let mut t = [0u64; 64];
    for sq in 0..64 {
        t[sq] = knight_attacks(1u64 << sq)
    }
    t
}

fn build_king_table() -> [u64; 64] {
    let mut t = [0u64; 64];
    for sq in 0..64 {
        t[sq] = king_attacks(1u64 << sq)
    }
    t
}
fn build_white_pawn() -> [u64; 64] {
    let mut t = [0u64; 64];
    for sq in 0..64 {
        t[sq] = pawn_attacks_white(1u64 << sq)
    }
    t
}
fn build_black_pawn() -> [u64; 64] {
    let mut t = [0u64; 64];
    for sq in 0..64 {
        t[sq] = pawn_attacks_black(1u64 << sq)
    }
    t
}
