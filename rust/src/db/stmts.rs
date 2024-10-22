use crate::ResultT;

use rusqlite::{Connection, Statement};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) struct Stmts<'a> {
    pub(super) insert_cvc5result: Rc<RefCell<Statement<'a>>>,
    pub(super) update_benchstatus: Rc<RefCell<Statement<'a>>>,
    pub(super) select_benchstatus: Rc<RefCell<Statement<'a>>>,
    pub(super) count_benchstatus: Rc<RefCell<Statement<'a>>>,
    pub(super) insert_source: Rc<RefCell<Statement<'a>>>,
    pub(super) insert_line: Rc<RefCell<Statement<'a>>>,
    pub(super) insert_line_usage: Rc<RefCell<Statement<'a>>>,
    pub(super) insert_function: Rc<RefCell<Statement<'a>>>,
    pub(super) insert_function_usage: Rc<RefCell<Statement<'a>>>,
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

        let insert_source = Rc::new(RefCell::new(
            conn.prepare("INSERT INTO \"sources\" ( path ) VALUES (?1)")
                .expect("Issue during sources insert query preparation"),
        ));

        let insert_line = Rc::new(RefCell::new(
            conn.prepare(
                "INSERT OR REPLACE INTO \"lines\" (
                source_id,
                line_no
            ) VALUES (?1, ?2)",
            )
            .expect("Issue during lines insert query preparation"),
        ));

        let insert_line_usage = Rc::new(RefCell::new(
            conn.prepare(
                "INSERT INTO \"usage_lines\" (
                    bench_id,
                    line_id,
                    usage
            ) VALUES (?1, ?2, ?3)",
            )
            .expect("Issue during usage_lines insert query preparation"),
        ));

        let insert_function = Rc::new(RefCell::new(
            conn.prepare(
                "INSERT OR REPLACE INTO \"functions\" (
                source_id,
                name,
                start_line,
                start_col,
                end_line,
                end_col
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .expect("Issue during functions insert query preparation"),
        ));

        let insert_function_usage = Rc::new(RefCell::new(
            conn.prepare(
                "INSERT INTO \"usage_functions\" (
                    bench_id,
                    func_id,
                    usage
            ) VALUES (?1, ?2, ?3)",
            )
            .expect("Issue during usage_functions insert query preparation"),
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
            insert_source: unsafe { std::mem::transmute(insert_source) },
            insert_line: unsafe { std::mem::transmute(insert_line) },
            insert_line_usage: unsafe { std::mem::transmute(insert_line_usage) },
            insert_function: unsafe { std::mem::transmute(insert_function) },
            insert_function_usage: unsafe { std::mem::transmute(insert_function_usage) },
        })
        // Ok(Stmts {
        //     insert_cvc5result,
        //     update_benchstatus,
        //     select_benchstatus,
        // })
    }
}
