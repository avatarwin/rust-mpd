// Hidden internal interface
#![allow(missing_docs)]

use std::io::{self, Read, Write, BufRead, Lines};
use std::collections::BTreeMap;
use std::fmt::Arguments;
use bufstream::BufStream;

use reply::Reply;
use error::{Result, Error, ProtoError};

pub struct Pairs<I>(pub I);

impl<I> Iterator for Pairs<I> where I: Iterator<Item=io::Result<String>> {
    type Item = Result<(String, String)>;
    fn next(&mut self) -> Option<Result<(String, String)>> {
        let reply: Option<Result<Reply>> = self.0.next().map(|v| v.map_err(Error::Io).and_then(|s| s.parse::<Reply>().map_err(Error::Parse)));
        match reply {
            Some(Ok(Reply::Pair(a, b))) => Some(Ok((a, b))),
            None | Some(Ok(Reply::Ok)) => None,
            Some(Ok(Reply::Ack(e))) => Some(Err(Error::Server(e))),
            Some(Err(e)) => Some(Err(e)),
        }
    }
}

struct Maps<'a, I: 'a> {
    pairs: &'a mut Pairs<I>,
    sep: &'a str,
    value: Option<String>,
    done: bool
}

impl<'a, I> Iterator for Maps<'a, I> where I: Iterator<Item=io::Result<String>> {
    type Item = Result<BTreeMap<String, String>>;
    fn next(&mut self) -> Option<Result<BTreeMap<String, String>>> {
        if self.done {
            return None;
        }

        let mut map = BTreeMap::new();

        if let Some(b) = self.value.take() {
            map.insert(self.sep.to_owned(), b);
        }

        loop {
            match self.pairs.next() {
                Some(Ok((a, b))) => {
                    if &*a == self.sep {
                        self.value = Some(b);
                        break;
                    } else {
                        map.insert(a, b);
                    }
                },
                Some(Err(e)) => return Some(Err(e)),
                None => {
                    self.done = true;
                    break;
                }
            }
        }

        Some(Ok(map))
    }
}

impl<I> Pairs<I> where I: Iterator<Item=io::Result<String>> {
    pub fn split<'a, 'b: 'a>(&'a mut self, f: &'b str) -> Maps<'a, I> {
        let mut maps = Maps {
            pairs: self,
            sep: f,
            value: None,
            done: false,
        };
        maps.next(); // swallow first separator
        maps
    }
}

// Client inner communication methods {{{
pub trait Proto {
    type Stream: Read+Write;

    fn read_line(&mut self) -> Result<String>;
    fn read_pairs(&mut self) -> Pairs<Lines<&mut BufStream<Self::Stream>>>;

    fn run_command(&mut self, command: &str) -> Result<()>;
    fn run_command_fmt(&mut self, command: Arguments) -> Result<()>;

    fn read_map(&mut self) -> Result<BTreeMap<String, String>> {
        self.read_pairs().collect()
    }

    fn drain(&mut self) -> Result<()> {
        loop {
            let reply = try!(self.read_line());
            match &*reply {
                "OK" | "list_OK" => break,
                _ => ()
            }
        }
        Ok(())
    }

    fn expect_ok(&mut self) -> Result<()> {
        let line = try!(self.read_line());

        match line.parse::<Reply>() {
            Ok(Reply::Ok) => Ok(()),
            Ok(Reply::Ack(e)) => Err(Error::Server(e)),
            Ok(_) => Err(Error::Proto(ProtoError::NotOk)),
            Err(e) => Err(From::from(e)),
        }
    }

    fn read_pair(&mut self) -> Result<(String, String)> {
        let line = try!(self.read_line());

        match line.parse::<Reply>() {
            Ok(Reply::Pair(a, b)) => Ok((a, b)),
            Ok(Reply::Ok) => Err(Error::Proto(ProtoError::NotPair)),
            Ok(Reply::Ack(e)) => Err(Error::Server(e)),
            Err(e) => Err(Error::Parse(e)),
        }
    }

    fn read_field(&mut self, field: &'static str) -> Result<String> {
        let (a, b) = try!(self.read_pair());
        if &*a == field {
            Ok(b)
        } else {
            Err(Error::Proto(ProtoError::NoField(field)))
        }
    }
}
// }}}
