use futures::stream::Stream as _;

mod input;

#[allow(clippy::type_complexity)]
struct Expect {
    process: tokio_pty_process_stream::Process<input::buf::Stdin>,
    expectations: Vec<(
        regex::Regex,
        Box<
            dyn Fn(&mut tokio_pty_process_stream::Process<input::buf::Stdin>)
                + Send,
        >,
    )>,
}

impl Expect {
    fn new(cmd: &str, args: &[String]) -> Self {
        Self {
            process: tokio_pty_process_stream::Process::new(
                cmd,
                args,
                input::buf::Stdin::new(),
            ),
            expectations: vec![],
        }
    }

    fn expect<
        F: Fn(&mut tokio_pty_process_stream::Process<input::buf::Stdin>)
            + Send
            + 'static,
    >(
        &mut self,
        rx: &str,
        cb: F,
    ) {
        self.expectations
            .push((regex::Regex::new(rx).unwrap(), Box::new(cb)));
    }
}

impl futures::future::Future for Expect {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        loop {
            let event = futures::try_ready!(self
                .process
                .poll()
                .map_err(|e| panic!(e)));
            match event {
                Some(tokio_pty_process_stream::Event::Output { data }) => {
                    let s = std::string::String::from_utf8_lossy(&data);
                    let mut found = None;
                    for (rx, cb) in &self.expectations {
                        if rx.is_match(&s) {
                            found = Some(cb);
                            break;
                        }
                    }
                    if let Some(cb) = found {
                        cb(&mut self.process);
                    }
                }
                Some(tokio_pty_process_stream::Event::CommandExit {
                    ..
                }) => break,
                None => break,
                _ => {}
            }
        }
        Ok(futures::Async::Ready(()))
    }
}

fn main() {
    tokio::run(futures::future::lazy(|| {
        let mut expect = Expect::new("nethack", &[]);
        expect.expect(r"Shall I pick.*[ynaq]", |process| {
            println!("shall i pick");
            process.input().send(b"n");
        });
        expect.expect(r"Pick a role", |process| {
            println!("pick a role");
            process.input().send(b"w");
        });
        expect.expect(r"Pick a race", |process| {
            println!("pick a race");
            process.input().send(b"e");
        });
        expect.expect(r"Pick a gender", |process| {
            println!("pick a gender");
            process.input().send(b"f");
        });
        expect.expect(r"start game", |process| {
            println!("start game");
            process.input().send(b"y");
        });
        expect.expect(r"welcome to NetHack", |process| {
            println!("welcome");
            process.input().send(b"#quit\n");
        });
        expect.expect(r"Really quit", |process| {
            println!("really quit");
            process.input().send(b"y");
        });
        expect.expect(
            r"Do you want your possessions identified",
            |process| {
                println!("dywypi");
                process.input().send(b"n");
            },
        );
        expect.expect(r"--More--", |process| {
            println!("more");
            process.input().send(b" ");
        });
        expect.expect(
            r"Do you want to see the dungeon overview",
            |process| {
                println!("dungeon overview");
                process.input().send(b"n");
            },
        );
        expect
    }));
}
