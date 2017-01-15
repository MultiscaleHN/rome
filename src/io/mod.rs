mod turtle;
mod ntriples_writer;

pub type TurtleParser<'a, B> = turtle::parser::TripleIterator<'a, B>;
// NTriples is a subset of Turtle
pub type NTriplesParser<'a, B> = turtle::parser::TripleIterator<'a, B>;
pub use self::turtle::turtle_writer::write_turtle;
pub use self::ntriples_writer::write_ntriples;
