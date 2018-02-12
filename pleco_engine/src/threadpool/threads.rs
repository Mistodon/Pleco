//! Contains the ThreadPool and the individual Threads.

// TODO: use `parking_lot::RwLock`
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool,Ordering};
use std::sync::mpsc::Sender;

use pleco::tools::pleco_arc::Arc;
use pleco::board::*;


use root_moves::RootMove;
use root_moves::root_moves_list::RootMoveList;
use root_moves::root_moves_manager::RmManager;
use sync::LockLatch;
use time::uci_timer::*;
use search::Searcher;

use super::{SendData, ThreadGo, TIMER};


/// The main execution thread of the pool. Technically a superset of the `Thread`
/// structure, but coordinates setting up and communicating between threads. This
/// is also the only point of contact between the actual search and the `ThreadPool`.
pub struct MainThread {
    pub per_thread: RmManager,
    pub main_thread_go: Arc<LockLatch>,
    pub sender: Sender<SendData>,
    pub thread: Thread,
    pub use_stdout: Arc<AtomicBool>
}

impl MainThread {
    pub fn main_idle_loop(&mut self) {
        while !self.thread.drop() {
            self.main_thread_go.wait();
            if self.thread.drop() {
                return;
            }
            self.go();
        }
    }

    pub fn lock_threads(&mut self) {
        self.thread.cond.lock();
    }

    pub fn start_threads(&mut self) {
        self.thread.cond.set();
    }

    pub fn lock_self(&mut self) {
        self.main_thread_go.lock();
    }

    pub fn go(&mut self) {
        self.per_thread.set_stop(true);
        self.per_thread.wait_for_finish();
        self.per_thread.reset_depths();
        let board = self.thread.retrieve_board().unwrap();
        unsafe {
            self.per_thread.replace_moves(&board);
        }
        // turn stop searching off
        // wakeup all threads

        self.per_thread.set_stop(false);
        let limit = self.thread.retrieve_limit().unwrap();

        // set the global timer and start the threads

        if let Some(timer) = limit.use_time_management() {
            TIMER.init(limit.start.clone(), &timer, board.turn(), board.moves_played());
        }
        self.start_threads();

        self.per_thread.wait_for_start();
        self.lock_threads();

        // start searching
        self.thread.start_searching(board, limit);
        self.per_thread.set_stop(true);
        self.per_thread.wait_for_finish();

        // find best move
        let best_root_move: RootMove = self.per_thread.best_rootmove(self.use_stdout.load(Ordering::Relaxed));

        self.sender.send(SendData::BestMove(best_root_move)).unwrap();

        if self.use_stdout.load(Ordering::Relaxed) {
            println!("bestmove {}", best_root_move.bit_move);
        }

        // return to idle loop
        self.lock_self();
    }
}

pub struct Thread {
    pub root_moves: RootMoveList,
    pub id: usize,
    pub pos_state: Arc<RwLock<Option<ThreadGo>>>,
    pub cond: Arc<LockLatch>,
    pub searcher: Searcher
}

impl Thread {
    pub fn drop(&self) -> bool {
        self.root_moves.get_kill()
    }

    pub fn stop(&self) -> bool {
        self.root_moves.load_stop()
    }

    pub fn retrieve_board(&self) -> Option<Board> {
        let s: &Option<ThreadGo> = &*(self.pos_state.read().unwrap());
        let board = s.as_ref().map(|ref tg| (*tg).board.shallow_clone());
        board
    }

    pub fn retrieve_limit(&self) -> Option<Limits> {
        let s: &Option<ThreadGo> = &*(self.pos_state.read().unwrap());
        let board = s.as_ref().map(|ref tg| (*tg).limit.clone());
        board
    }

    pub fn idle_loop(&mut self) {
        while !self.drop(){
            self.cond.wait();
            if self.drop() {
                return;
            }
            self.go();
        }
    }

    fn start_searching(&mut self, board: Board, limit: Limits) {
        self.searcher.limit = limit;
        self.searcher.board = board;
        self.searcher.search_root();
    }

    pub fn go(&mut self) {
        let board = self.retrieve_board().unwrap();
        let limit = self.retrieve_limit().unwrap();
        self.start_searching(board, limit);
    }

}