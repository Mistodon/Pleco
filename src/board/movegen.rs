//! Module for generating moves from a [Board]. Allow for generating Legal and Pseudo-Legal moves
//! of various types.
//!
//! # Generation Types
//!
//! The Types of moves that can be generated are:
//!
//! `All`, `Captures`, `Quiets`, `QuietChecks`, `Evasions`, `NonEvasions`
//!
//! Generating all moves is legal to do no matter the position. However, `Captures`, `Quiets`,
//! `QuietChecks`, and `NonEvasions` can only be done if the board is in NOT in check. Likewise,
//! `Evasions` can only be done when the board is currently in check.
//!
//! # `Legal` vs. `PseudoLegal` Moves
//!
//! For the generation type, moves can either be generated to be Legal, Or Pseudo-Legal. A Legal
//! move is, for as the name implies, a legal move for the current side to play for a given position.
//! A Pseudo-Legal move is a move that is "likely" to be legal for the current position, but cannot
//! be gaurnteed.
//!
//! Why would someone ever want to generate moves that might not be legal? Performance. Based on
//! some benchmarking, generating all Pseudo-Legal moves is around twice as fast as generating all
//! Legal moves. So, if you are fine with generating moves and then checking them post-generation
//! with a `Board::is_legal(m: BitMove)`, then the performance boost is potentially worth it.

use core::templates::*;
use board::*;
use core::piece_move::{MoveFlag, BitMove, PreMoveInfo};
use core::bit_twiddles::*;
use core::magic_helper::MagicHelper;

//                   Legal    PseudoLegal
//         All:  10,172 ns  |  9,636 ns
// NonEvasions:   8,381 ns  |  4,179 ns
//    Captures:   2,491 ns  |  2,230 ns
//      Quiets:   2,491 ns  |  4,506 n
// QuietChecks:   7,988 ns  |  3,411 ns
//    Evasions:   4,034 ns  |  2,689 ns
//
//
//      With Full Player MonoMorphization
//
//                   Legal    PseudoLegal
//         All:   9,275 ns  |  4,814 ns
// NonEvasions:   8,421 ns  |  4,179 ns
//    Captures:   2,550 ns  |  2,230 ns
//      Quiets:   2,491 ns  |  4,506 n
// QuietChecks:   6,124 ns  |  3,411 ns
//    Evasions:   3,930 ns  |  2,649 ns
//
// With Full Player MonoMorphization



/// Determines the if the moves generated are `PseudoLegal` or `Legal` moves.
/// PseudoLegal moves require that a move's legality is determined before applying
/// to a `Board`.
pub trait Legality {
    fn gen_legal() -> bool;
}

/// Dummy Struct to represent the generation of `Legal` Moves.
pub struct Legal {}

/// Dummy Struct to represent the generation of `PseudoLegal` Moves.
pub struct PseudoLegal {}

impl Legality for Legal {
    fn gen_legal() -> bool {
        true
    }
}

impl Legality for PseudoLegal {
    fn gen_legal() -> bool {
        false
    }
}


// Pieces to generate moves with inter-changably
const STANDARD_PIECES: [Piece; 4] = [Piece::B, Piece::N, Piece::R, Piece::Q];

/// Structure to generate moves from. Stores the current state of the board, and other
/// references to help generating all possible moves.
pub struct MoveGen<'a> {
    movelist: Vec<BitMove>,
    board: &'a Board,
    magic: &'static MagicHelper<'static, 'static>,
    occ: BitBoard, // Squares occupied by all
    us_occ: BitBoard, // squares occupied by player to move
    them_occ: BitBoard, // Squares occupied by the opposing player
}

impl<'a> MoveGen<'a> {

    // Helper function to setup the MoveGen structure
    fn get_self(chessboard: &'a Board) -> Self {
        MoveGen {
            movelist: Vec::with_capacity(48),
            board: &chessboard,
            magic: chessboard.magic_helper,
            occ: chessboard.get_occupied(),
            us_occ: chessboard.get_occupied_player(chessboard.turn()),
            them_occ: chessboard.get_occupied_player(chessboard.turn().other_player()),
        }
    }

    /// Returns vector of all moves for a given board, Legality & GenType.
    pub fn generate<L: Legality, G: GenTypeTrait>(chessboard: &Board) -> Vec<BitMove> {
        match chessboard.turn() {
            Player::White => MoveGen::generate_helper::<L,G, WhiteType>(&chessboard),
            Player::Black => MoveGen::generate_helper::<L,G, BlackType>(&chessboard)
        }
    }

    fn generate_helper<L: Legality, G: GenTypeTrait, P: PlayerTrait>(chessboard: &Board) -> Vec<BitMove> {
        let mut movegen = MoveGen::get_self(&chessboard);
        let gen_type = G::gen_type();
        if gen_type == GenTypes::Evasions {
            movegen.generate_evasions::<L,P>();
        } else if gen_type == GenTypes::QuietChecks {
            movegen.generate_quiet_checks::<L,P>();
        } else  {
            if gen_type == GenTypes::All {
                if movegen.board.in_check() {
                    movegen.generate_evasions::<L,P>();
                } else {
                    movegen.generate_non_evasions::<L, NonEvasionsGenType,P>();
                }
            } else {
                movegen.generate_non_evasions::<L,G,P>();
            }
        }
        movegen.movelist
    }

    fn generate_non_evasions<L: Legality, G: GenTypeTrait, P: PlayerTrait>(&mut self) {
        assert_ne!(G::gen_type(), GenTypes::All);
        assert_ne!(G::gen_type(), GenTypes::QuietChecks);
        assert_ne!(G::gen_type(), GenTypes::Evasions);
        assert!(!self.board.in_check());

        // target = Bitboard of squares the generator should aim for
        let target: BitBoard = match G::gen_type() {
            GenTypes::NonEvasions => !self.us_occ,
            GenTypes::Captures => self.them_occ,
            GenTypes::Quiets => !(self.us_occ | self.them_occ),
            _ => unreachable!()
        };

        self.generate_all::<L, G, P>(target);
    }

    fn generate_all<L: Legality, G: GenTypeTrait, P: PlayerTrait>(&mut self, target: BitBoard) {
        self.generate_pawn_moves::<L, G, P>(target);
        self.moves_per_piece::<L, P, KnightType>(target);
        self.moves_per_piece::<L, P, BishopType>(target);
        self.moves_per_piece::<L, P, RookType>(target);
        self.moves_per_piece::<L, P ,QueenType>(target);

        if G::gen_type() != GenTypes::QuietChecks && G::gen_type() != GenTypes::Evasions {
            self.generate_king_moves::<L, P>(target);
        }

        if G::gen_type() != GenTypes::Captures && G::gen_type() != GenTypes::Evasions {
            self.generate_castling::<L, P>();
        }

    }

    fn generate_quiet_checks<L: Legality, P: PlayerTrait>(&mut self) {
        assert!(!self.board.in_check());
        let mut disc_check: BitBoard = self.board.discovered_check_candidates();

        while disc_check != 0 {
            let dc_lsb: BitBoard = lsb(disc_check);
            let from: SQ = bb_to_sq(dc_lsb);
            disc_check &= !dc_lsb;
            let piece: Piece = self.board.piece_at_sq(from).unwrap();
            if piece != Piece::P {
                let mut b: BitBoard = self.moves_bb(piece, from) & !self.board.get_occupied();
                if piece == Piece::K {
                    b &= self.magic.queen_moves(0,self.board.king_sq(P::opp_player()))
                }
                self.move_append_from_bb::<L>(&mut b, from, MoveFlag::QuietMove);
            }
        }
        self.generate_all::<L, QuietChecksGenType, P>(!self.board.get_occupied());
    }


    // Helper function to generate evasions
    fn generate_evasions<L: Legality, P: PlayerTrait>(&mut self) {
        assert!(self.board.in_check());

        let ksq: SQ = self.board.king_sq(P::player());
        let mut slider_attacks: BitBoard = 0;

        // Pieces that could possibly attack the king with sliding attacks
        let mut sliders = self.board.checkers() & !self.board.piece_two_bb_both_players(Piece::P, Piece::N);

        // This is getting all the squares that are attacked by sliders
        while sliders != 0 {
            let check_sq_bb: BitBoard = lsb(sliders);
            let check_sq: SQ = bb_to_sq(check_sq_bb);
            slider_attacks |= self.magic.line_bb(check_sq, ksq) ^ check_sq_bb;
            sliders &= !check_sq_bb;
        }

        // Possible king moves, Where the king cannot move into a slider / own pieces
        let k_moves: BitBoard = self.magic.king_moves(ksq) & !slider_attacks & !self.us_occ;

        // Seperate captures and non captures
        let mut captures_bb: BitBoard = k_moves & self.them_occ;
        let mut non_captures_bb: BitBoard = k_moves & !self.them_occ;
        self.move_append_from_bb::<L>(&mut captures_bb, ksq, MoveFlag::Capture { ep_capture: false },
        );
        self.move_append_from_bb::<L>(&mut non_captures_bb, ksq, MoveFlag::QuietMove);

        // If there is only one checking square, we can block or capture the piece
        if !more_than_one(self.board.checkers()) {
            let checking_sq: SQ = bit_scan_forward(self.board.checkers());

            // Squares that allow a block or capture of the sliding piece
            let target: BitBoard = self.magic.between_bb(checking_sq, ksq) | sq_to_bb(checking_sq);
            self.generate_all::<L, EvasionsGenType, P>(target);
        }
    }

    // Generate king moves with a given target
    fn generate_king_moves<L: Legality, P: PlayerTrait>(&mut self, target: BitBoard) {
        self.moves_per_piece::<L, P, KingType>(target);
    }

    // Generates castling for both sides
    fn generate_castling<L: Legality, P: PlayerTrait>(&mut self) {
        self.castling_side::<L, P>(CastleType::QueenSide);
        self.castling_side::<L, P>(CastleType::KingSide);
    }

    // Generates castling for a single side
    fn castling_side<L: Legality, P: PlayerTrait>(&mut self, side: CastleType) {
        // Make sure we can castle AND the space between the king / rook is clear AND the piece at castling_side is a Rook
        if !self.board.castle_impeded(side) && self.board.can_castle(P::player(), side) &&
            self.board
                .piece_at_sq(self.board.castling_rook_square(side)) == Some(Piece::R)
        {

            let king_side: bool = { side == CastleType::KingSide };

            let ksq: SQ = self.board.king_sq(P::player());
            let r_from: SQ = self.board.castling_rook_square(side);
            let k_to = P::player().relative_square(
                if king_side {
                    Square::G1 as SQ
                } else {
                    Square::C1 as SQ
                },
            );

            let enemies: BitBoard = self.them_occ;
            let direction: fn(SQ) -> SQ = if king_side {
                |x: SQ| x.wrapping_sub(1)
            } else {
                |x: SQ| x.wrapping_add(1)
            };

            let mut s: SQ = k_to;
            let mut can_castle: bool = true;

            // Loop through all the squares the king goes through
            // If any enemies attack that square, cannot castle
            'outer: while s != ksq {
                let attackers = self.board.attackers_to(s, self.occ) & enemies;
                if attackers != 0 {
                    can_castle = false;
                    break 'outer;
                }
                s = direction(s);
            }
            if can_castle {
                self.check_and_add::<L>(BitMove::init(PreMoveInfo {
                    src: ksq,
                    dst: r_from,
                    flags: MoveFlag::Castle { king_side: king_side },
                }));
            }

        }
    }

    // Generate non-pawn and non-king moves for a target
    fn gen_non_pawn_king<L: Legality, P: PlayerTrait>(&mut self, target: BitBoard) {
        self.moves_per_piece::<L, P, KnightType>(target);
        self.moves_per_piece::<L, P, BishopType>(target);
        self.moves_per_piece::<L, P, RookType>(target);
        self.moves_per_piece::<L, P ,QueenType>(target);
    }


    // Get the captures and non-captures for a piece
    fn moves_per_piece<L: Legality, PL: PlayerTrait, P: PieceTrait>(&mut self, target: BitBoard) {
        let mut piece_bb: BitBoard = self.board.piece_bb(PL::player(), P::piece_type());
        while piece_bb != 0 {
            let b: BitBoard = lsb(piece_bb);
            let src: SQ = bb_to_sq(b);
            let moves_bb: BitBoard = self.moves_bb(P::piece_type(), src) & !self.us_occ & target;
            let mut captures_bb: BitBoard = moves_bb & self.them_occ;
            let mut non_captures_bb: BitBoard = moves_bb & !self.them_occ;
            self.move_append_from_bb::<L>(
                &mut captures_bb,
                src,
                MoveFlag::Capture { ep_capture: false },
            );
            self.move_append_from_bb::<L>(&mut non_captures_bb, src, MoveFlag::QuietMove);
            piece_bb &= !b;
        }
    }

    // Generate pawn moves
    fn generate_pawn_moves<L: Legality, G: GenTypeTrait, P: PlayerTrait>(&mut self, target: BitBoard) {

        let (rank_8, rank_7, rank_3): (BitBoard, BitBoard, BitBoard) = if P::player() == Player::White {
            (RANK_8, RANK_7, RANK_3)
        } else {
            (RANK_1, RANK_2, RANK_6)
        };

        let all_pawns: BitBoard = self.board.piece_bb(P::player(), Piece::P);

        // seperate these two for promotion moves and non promotions
        let pawns_rank_7: BitBoard = all_pawns & rank_7;
        let pawns_not_rank_7: BitBoard = all_pawns & !rank_7;

        let mut empty_squares: BitBoard = 0;

        let enemies: BitBoard = if G::gen_type() == GenTypes::Evasions {
            self.them_occ & target
        } else if G::gen_type() == GenTypes::Captures {
            target
        } else {
            self.them_occ
        };

        // Single and Double Pawn Pushes
        if G::gen_type() != GenTypes::Captures {
            empty_squares =
                if G::gen_type() == GenTypes::Quiets || G::gen_type() == GenTypes::QuietChecks {
                    target
                } else {
                    !self.board.get_occupied()
                };

            let mut push_one: BitBoard = empty_squares & P::shift_up(pawns_not_rank_7);
            // double pushes are pawns that can be pushed one and remain on rank3
            let mut push_two: BitBoard = P::shift_up(push_one & rank_3) & empty_squares;

            if G::gen_type() == GenTypes::Evasions {
                push_one &= target;
                push_two &= target;
            }

            if G::gen_type() == GenTypes::QuietChecks {
                let ksq: SQ = self.board.king_sq(P::opp_player());
                push_one &= self.magic.pawn_attacks_from(ksq, P::opp_player());
                push_two &= self.magic.pawn_attacks_from(ksq, P::opp_player());

                let dc_candidates: BitBoard = self.board.discovered_check_candidates();
                if pawns_not_rank_7 & dc_candidates != 0 {
                    let dc1: BitBoard = P::shift_up(pawns_not_rank_7 & dc_candidates) &
                        empty_squares & !file_bb(ksq);
                    let dc2: BitBoard = P::shift_up(rank_3 & dc1) & empty_squares;

                    push_one |= dc1;
                    push_two |= dc2;
                }
            }

            while push_one != 0 {
                let bit: BitBoard = lsb(push_one);
                let dst: SQ = bb_to_sq(bit);
                let src: SQ = P::down(dst);
                self.check_and_add::<L>(BitMove::init(PreMoveInfo {
                    src: src,
                    dst: dst,
                    flags: MoveFlag::QuietMove,
                }));
                push_one &= !bit;
            }

            while push_two != 0 {
                let bit: BitBoard = lsb(push_two);
                let dst: SQ = bb_to_sq(bit);
                let src: SQ = P::down(P::down(dst));
                self.check_and_add::<L>(BitMove::init(PreMoveInfo {
                    src: src,
                    dst: dst,
                    flags: MoveFlag::DoublePawnPush,
                }));
                push_two &= !bit;
            }
        }

        // Promotions
        if pawns_rank_7 != 0 && (G::gen_type() != GenTypes::Evasions || (target & rank_8) != 0) {
            if G::gen_type() == GenTypes::Captures {
                empty_squares = !self.occ;
            } else if G::gen_type() == GenTypes::Evasions {
                empty_squares &= target;
            }

            let mut no_promo: BitBoard = P::shift_up(pawns_rank_7) & empty_squares;
            let mut left_cap_promo: BitBoard = P::shift_up_left(pawns_rank_7) & enemies;
            let mut right_cap_promo: BitBoard = P::shift_up_right(pawns_rank_7) & enemies;

            while no_promo != 0 {
                let bit = lsb(no_promo);
                let dst: SQ = bb_to_sq(bit);
                self.create_all_promotions::<L>(dst, P::down(dst), false);
                no_promo &= !bit;
            }

            while left_cap_promo != 0 {
                let bit = lsb(left_cap_promo);
                let dst: SQ = bb_to_sq(bit);
                self.create_all_promotions::<L>(dst, P::down_right(dst), true);
                left_cap_promo &= !bit;
            }

            while right_cap_promo != 0 {
                let bit = lsb(right_cap_promo);
                let dst: SQ = bb_to_sq(bit);
                self.create_all_promotions::<L>(dst, P::down_left(dst), true);
                right_cap_promo &= !bit;
            }
        }

        // Captures
        if G::gen_type() == GenTypes::Captures || G::gen_type() == GenTypes::Evasions ||
            G::gen_type() == GenTypes::NonEvasions || G::gen_type() == GenTypes::All
        {

            let mut left_cap: BitBoard = P::shift_up_left(pawns_not_rank_7) & enemies;
            let mut right_cap: BitBoard = P::shift_up_right(pawns_not_rank_7) & enemies;

            while left_cap != 0 {
                let bit = lsb(left_cap);
                let dst: SQ = bb_to_sq(bit);
                let src: SQ = P::down_right(dst);
                self.check_and_add::<L>(BitMove::init(PreMoveInfo {
                    src: src,
                    dst: dst,
                    flags: MoveFlag::Capture { ep_capture: false },
                }));
                left_cap &= !bit;
            }

            while right_cap != 0 {
                let bit = lsb(right_cap);
                let dst: SQ = bb_to_sq(bit);
                let src: SQ = P::down_left(dst);
                self.check_and_add::<L>(BitMove::init(PreMoveInfo {
                    src: src,
                    dst: dst,
                    flags: MoveFlag::Capture { ep_capture: false },
                }));
                right_cap &= !bit;
            }

            if self.board.ep_square() != NO_SQ {
                let ep_sq: SQ = self.board.ep_square();
                assert_eq!(rank_of_sq(ep_sq), relative_rank(P::player(), Rank::R6));
                if G::gen_type() != GenTypes::Evasions || target & sq_to_bb(P::down(ep_sq)) != 0 {
                    left_cap = pawns_not_rank_7 & self.magic.pawn_attacks_from(ep_sq, P::opp_player());

                    while left_cap != 0 {
                        let bit = lsb(left_cap);
                        let src: SQ = bb_to_sq(bit);
                        self.check_and_add::<L>(BitMove::init(PreMoveInfo {
                            src: src,
                            dst: ep_sq,
                            flags: MoveFlag::Capture { ep_capture: true },
                        }));
                        left_cap &= !bit;
                    }
                }
            }
        }
    }

    // Helper function for creating promotions
    #[inline]
    fn create_all_promotions<L: Legality>(&mut self, dst: SQ, src: SQ, is_capture: bool) {
        let prom_pieces = [Piece::Q, Piece::N, Piece::R, Piece::B];
        for piece in &prom_pieces {
            if is_capture {
                self.check_and_add::<L>(BitMove::init(PreMoveInfo {
                    src: src,
                    dst: dst,
                    flags: MoveFlag::Promotion {
                        capture: true,
                        prom: *piece,
                    },
                }));
            } else {
                self.check_and_add::<L>(BitMove::init(PreMoveInfo {
                    src: src,
                    dst: dst,
                    flags: MoveFlag::Promotion {
                        capture: false,
                        prom: *piece,
                    },
                }));
            }
        }
    }

    // Return the moves Bitboard
    #[inline]
    fn moves_bb(&self, piece: Piece, square: SQ) -> BitBoard {
        assert!(sq_is_okay(square));
        assert_ne!(piece, Piece::P);
        match piece {
            Piece::P => panic!(),
            Piece::N => self.magic.knight_moves(square),
            Piece::B => self.magic.bishop_moves(self.occ, square),
            Piece::R => self.magic.rook_moves(self.occ, square),
            Piece::Q => self.magic.queen_moves(self.occ, square),
            Piece::K => self.magic.king_moves(square),
        }
    }

    #[inline]
    fn move_append_from_bb<L: Legality>(&mut self, bits: &mut BitBoard, src: SQ, move_flag: MoveFlag) {
        while *bits != 0 {
            let bit: BitBoard = lsb(*bits);
            let b_move = BitMove::init(PreMoveInfo {
                src: src,
                dst: bb_to_sq(bit),
                flags: move_flag,
            });
            self.check_and_add::<L>(b_move);
            *bits &= !bit;
        }
    }

    /// Checks if the move is legal, and if so adds to the move list.
    #[inline]
    fn check_and_add<L: Legality>(&mut self, b_move: BitMove) {
        if L::gen_legal() {
            if self.board.legal_move(b_move) {
                self.movelist.push(b_move);
            }
        } else {
            self.movelist.push(b_move);
        }
    }
}