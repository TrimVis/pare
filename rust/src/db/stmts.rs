use crate::ResultT;

use rusqlite::{Connection, Statement};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) struct Stmts<'a> {
    pub(super) insert_cvc5result: Rc<RefCell<Statement<'a>>>,
    pub(super) update_benchstatus: Rc<RefCell<Statement<'a>>>,
    pub(super) select_benchstatus: Rc<RefCell<Statement<'a>>>,
    pub(super) count_benchstatus: Rc<RefCell<Statement<'a>>>,
    pub(super) count_benchstatus_total: Rc<RefCell<Statement<'a>>>,
}
impl<'a> Stmts<'a> {
    pub(super) fn new(conn: Rc<RefCell<Connection>>) -> ResultT<Self> {
        let conn = conn.borrow_mut();
        // TODO: Update expect messages
        let insert_cvc5result = Rc::new(RefCell::new(
            conn.prepare(
                "INSERT INTO \"result_benchmarks\" (
                bench_id,
                time_ms,
                exit_code,
                stdout,
                stderr
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .expect("Issue during benchmark status update query preparation"),
        ));

        let update_benchstatus = Rc::from(RefCell::new(
            conn.prepare(
                "INSERT INTO \"status_benchmarks\" (bench_id, status) 
                VALUES (?1, ?2) 
                ON CONFLICT(bench_id) DO UPDATE SET status = ?2",
            )
            .expect("Issue during status benchmark udpate query preparation"),
        ));

        let select_benchstatus = Rc::from(RefCell::new(
            conn.prepare(
                "SELECT s.bench_id, b.path, b.prefix
                FROM \"status_benchmarks\" AS s
                JOIN \"benchmarks\" AS b ON b.id = s.bench_id
                WHERE s.status = ?1
                LIMIT ?2",
            )
            .expect("Issue during benchstatus select query preparation"),
        ));

        let count_benchstatus_total = Rc::from(RefCell::new(
            conn.prepare(
                "SELECT COUNT(1)
                FROM \"status_benchmarks\" AS s
                JOIN \"benchmarks\" AS b ON b.id = s.bench_id",
            )
            .expect("Issue during benchstatus count query preparation"),
        ));

        let count_benchstatus = Rc::from(RefCell::new(
            conn.prepare(
                "SELECT COUNT(1)
                FROM \"status_benchmarks\" AS s
                JOIN \"benchmarks\" AS b ON b.id = s.bench_id
                WHERE s.status = ?1",
            )
            .expect("Issue during benchstatus count query preparation"),
        ));

        // FIXME: Dirty hack by ChatGPT. There is likely a better way
        Ok(Stmts {
            insert_cvc5result: unsafe { std::mem::transmute(insert_cvc5result) },
            update_benchstatus: unsafe { std::mem::transmute(update_benchstatus) },
            select_benchstatus: unsafe { std::mem::transmute(select_benchstatus) },
            count_benchstatus: unsafe { std::mem::transmute(count_benchstatus) },
            count_benchstatus_total: unsafe { std::mem::transmute(count_benchstatus_total) },
        })
        // Ok(Stmts {
        //     insert_cvc5result,
        //     update_benchstatus,
        //     select_benchstatus,
        // })
    }
}
