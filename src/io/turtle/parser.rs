use super::grammar_structs::*;
use super::grammar::{statement, tws};
use super::grammar_helper::*;
use std::marker::PhantomData;
use std::collections::HashMap;
use nom::IResult;
use graph;
use namespaces::*;
use std::rc::Rc;
use error::{Error, Result};
use regex::Regex;

struct StatementIterator<'a> {
    src: &'a str,
    done: bool,
}

impl<'a> StatementIterator<'a> {
    pub fn new(src: &str) -> Result<StatementIterator> {
        match tws(src) {
            IResult::Done(left, _) => {
                Ok(StatementIterator {
                    src: left,
                    done: false,
                })
            }
            IResult::Error(_) => return Err(Error::Custom("cannot start parsing")),
            IResult::Incomplete(_) => {
                Ok(StatementIterator {
                    src: src,
                    done: false,
                })
            }
        }
    }
}

impl<'a> Iterator for StatementIterator<'a> {
    type Item = Result<Statement<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let mut r;
        match statement(self.src) {
            IResult::Done(left, s) => {
                r = Some(Ok(s));
                self.src = left;
            }
            IResult::Error(e) => {
                r = Some(Err(Error::from(e)));
                self.done = true;
            }
            IResult::Incomplete(_) => {
                self.done = true;
                r = None;
            }
        }
        match tws(self.src) {
            IResult::Done(left, _) => {
                self.src = left;
            }
            IResult::Error(_) => {
                r = Some(Err(Error::Custom("error parsing whitespace")));
                self.done = true;
            }
            IResult::Incomplete(_) => {
                self.done = true;
            }
        }
        if r.is_none() && self.src.len() > 0 {
            r = Some(Err(Error::Custom("trailing bytes")));
        }
        r
    }
}

struct BlankNodes<'a> {
    blank_nodes: HashMap<&'a str, usize>,
    next_blank: usize,
}

#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Debug)]
pub struct BlankNodePtr<'g> {
    graph_id: u32,
    node_id: u32,
    phantom: PhantomData<&'g u32>,
}
impl<'g> graph::BlankNodePtr<'g> for BlankNodePtr<'g> {
    fn graph_id(&self) -> u32 {
        self.graph_id
    }
    fn node_id(&self) -> u32 {
        self.node_id
    }
}
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord,Debug)]
pub struct IRIPtr<'g> {
    iri: Rc<String>,
    phantom: PhantomData<&'g u32>,
}
impl<'g> graph::IRIPtr<'g> for IRIPtr<'g> {
    fn as_str(&self) -> &str {
        self.iri.as_str()
    }
}
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord,Debug)]
pub struct LiteralPtr<'g> {
    pub lexical: Rc<String>,
    pub datatype: Rc<String>,
    pub language: Option<Rc<String>>,
    phantom: PhantomData<&'g u32>,
}
impl<'g> graph::LiteralPtr<'g> for LiteralPtr<'g> {
    fn as_str(&self) -> &str {
        self.lexical.as_str()
    }
    fn datatype(&self) -> &str {
        self.datatype.as_str()
    }
    fn language(&self) -> Option<&str> {
        match &self.language {
            &Some(ref l) => Some(l.as_str()),
            &None => None,
        }
    }
}
type BlankNodeOrIRI<'t> = graph::BlankNodeOrIRI<'t, BlankNodePtr<'t>, IRIPtr<'t>>;
type Resource<'t> = graph::Resource<'t, BlankNodePtr<'t>, IRIPtr<'t>, LiteralPtr<'t>>;

#[derive(PartialEq,Eq)]
pub struct IteratorTriple<'t> {
    pub subject: BlankNodeOrIRI<'t>,
    pub predicate: IRIPtr<'t>,
    pub object: Resource<'t>,
}
impl<'t> graph::Triple<'t, BlankNodePtr<'t>, IRIPtr<'t>, LiteralPtr<'t>> for IteratorTriple<'t> {
    fn subject(&self) -> BlankNodeOrIRI<'t> {
        self.subject.clone()
    }
    fn predicate(&self) -> IRIPtr<'t> {
        self.predicate.clone()
    }
    fn object(&self) -> Resource<'t> {
        self.object.clone()
    }
}
struct Strings {
    subject: Rc<String>,
    predicate: Rc<String>,
    object: Rc<String>,
    datatype: Rc<String>,
    language: Rc<String>,
}

pub struct TripleIterator<'a> {
    statement_iterator: StatementIterator<'a>,
    buffer: String,
    base: String,
    prefixes: Namespaces,
    blank_nodes: BlankNodes<'a>,
    triple_buffer: Vec<Triple<'a>>,
    done: bool,
    strings: Strings,
}

fn is_absolute(url: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new("^[a-z][a-z0-9+.-]*:").unwrap();
    }
    RE.is_match(url)
}

fn join_iri(base: &str, p: &str, to: &mut String) -> Result<()> {
    to.clear();
    if !is_absolute(p) {
        let mut end = base.len();
        if !p.starts_with("#") {
            if let Some(pos) = base.rfind('/') {
                end = pos + 1;
            }
        }
        to.push_str(&base[..end]);
    }
    to.push_str(p);
    Ok(())
}

impl<'a> TripleIterator<'a> {
    pub fn new(src: &'a str, base: &str) -> Result<TripleIterator<'a>> {
        if !is_absolute(base) {
            return Err(Error::Custom("base url is not absolute"));
        }
        let rc = Rc::new(String::new());
        Ok(TripleIterator {
            statement_iterator: StatementIterator::new(src)?,
            buffer: String::new(),
            base: String::from(base),
            prefixes: Namespaces::new(),
            blank_nodes: BlankNodes {
                blank_nodes: HashMap::new(),
                next_blank: 0,
            },
            triple_buffer: Vec::new(),
            done: false,
            strings: Strings {
                subject: rc.clone(),
                predicate: rc.clone(),
                object: rc.clone(),
                datatype: rc.clone(),
                language: rc.clone(),
            },
        })
    }
    pub fn prefixes(&self) -> &Namespaces {
        &self.prefixes
    }
    fn set_prefix(&mut self, prefix: &'a str, value: String) {
        self.prefixes.insert(prefix.as_bytes(), value);
    }
    fn fill_buffer(&mut self) -> Result<usize> {
        while let Some(statement) = self.statement_iterator.next() {
            match statement {
                Ok(Statement::Prefix(prefix, iri)) => {
                    let mut result = String::with_capacity(iri.len());
                    self.buffer.clear();
                    unescape_iri(iri, &mut self.buffer)?;
                    join_iri(self.base.as_str(), self.buffer.as_str(), &mut result)?;
                    self.set_prefix(prefix, result);
                }
                Ok(Statement::Base(new_base)) => {
                    self.buffer.clear();
                    unescape_iri(new_base, &mut self.buffer)?;
                    let old_base = self.base.clone();
                    join_iri(old_base.as_str(), self.buffer.as_str(), &mut self.base)?;
                }
                Ok(Statement::Triples(new_triples)) => {
                    add_triples(new_triples, &mut self.blank_nodes, &mut self.triple_buffer)?;
                    return Ok(self.triple_buffer.len());
                }
                Err(e) => return Err(e),
            }
        }
        Ok(0)
    }
}

fn resolve_triple<'g>(triple: Triple,
                      prefixes: &Namespaces,
                      base: &str,
                      buffer: &mut String,
                      strings: &mut Strings)
                      -> Result<IteratorTriple<'g>> {
    Ok(IteratorTriple {
        subject: match triple.subject {
            SingleSubject::IRI(iri) => {
                resolve_iri(iri, prefixes, base, buffer, &mut strings.subject)?;
                graph::BlankNodeOrIRI::IRI(IRIPtr {
                    iri: strings.subject.clone(),
                    phantom: PhantomData,
                })
            }
            SingleSubject::BlankNode(n) => {
                graph::BlankNodeOrIRI::BlankNode(BlankNodePtr {
                                                     node_id: n as u32,
                                                     graph_id: 0,
                                                     phantom: PhantomData,
                                                 },
                                                 PhantomData)
            }
        },
        predicate: {
            resolve_iri(triple.predicate,
                        prefixes,
                        base,
                        buffer,
                        &mut strings.predicate)?;
            IRIPtr {
                iri: strings.predicate.clone(),
                phantom: PhantomData,
            }
        },
        object: match triple.object {
            SingleObject::IRI(iri) => {
                resolve_iri(iri, prefixes, base, buffer, &mut strings.object)?;
                graph::Resource::IRI(IRIPtr {
                    iri: strings.object.clone(),
                    phantom: PhantomData,
                })
            }
            SingleObject::BlankNode(n) => {
                graph::Resource::BlankNode(BlankNodePtr {
                                               node_id: n as u32,
                                               graph_id: 0,
                                               phantom: PhantomData,
                                           },
                                           PhantomData)
            }
            SingleObject::Literal(l) => {
                graph::Resource::Literal(LiteralPtr {
                    lexical: {
                        unescape_literal(l.lexical, &mut strings.object)?;
                        strings.object.clone()
                    },
                    datatype: {
                        resolve_iri(l.datatype, prefixes, base, buffer, &mut strings.datatype)?;
                        strings.datatype.clone()
                    },
                    language: match l.language {
                        Some(l) => {
                            {
                                let s = Rc::make_mut(&mut strings.language);
                                s.clear();
                                for c in l.chars() {
                                    s.extend(c.to_lowercase());
                                }
                            }
                            Some(strings.language.clone())
                        }
                        None => None,
                    },
                    phantom: PhantomData,
                })
            }
        },
    })
}
fn unescape_literal(string: &str, to: &mut Rc<String>) -> Result<()> {
    let p = Rc::make_mut(to);
    p.clear();
    unescape(string, p)?;
    Ok(())
}
fn resolve_iri(iri: IRI,
               prefixes: &Namespaces,
               base: &str,
               buffer: &mut String,
               to: &mut Rc<String>)
               -> Result<()> {
    let p = Rc::make_mut(to);
    p.clear();
    match iri {
        IRI::IRI(iri) => {
            buffer.clear();
            unescape_iri(iri, buffer)?;
            join_iri(base, buffer.as_str(), p)?;
        }
        IRI::PrefixedName(prefix, local) => {
            match prefixes.find_namespace(prefix.as_bytes()) {
                Some(ns) => {
                    p.push_str(ns);
                    pn_local_unescape(local, p)?;
                }
                None => return Err(Error::Custom("Cannot find prefix.")),
            }
        }
    }
    Ok(())
}

impl<'a> BlankNodes<'a> {
    fn new_blank(&mut self) -> usize {
        let b = self.next_blank;
        self.next_blank += 1;
        b
    }
    fn get_blank(&mut self, label: &'a str) -> usize {
        if let Some(n) = self.blank_nodes.get(label) {
            return *n;
        }
        let n = self.new_blank();
        self.blank_nodes.insert(label, n);
        n
    }
}

impl<'a> Iterator for TripleIterator<'a> {
    type Item = Result<IteratorTriple<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if self.triple_buffer.len() == 0 {
            match self.fill_buffer() {
                Ok(0) => {
                    self.done = true;
                    return None;
                }
                Ok(_) => {}
                Err(e) => return Some(Err(e)),
            }
        }
        match self.triple_buffer.pop() {
            Some(t) => {
                Some(resolve_triple(t,
                                    &self.prefixes,
                                    &self.base,
                                    &mut self.buffer,
                                    &mut self.strings))
            }
            None => None,
        }
    }
}

fn make_blank<'a>(blank_node: BlankNode<'a>, blank_nodes: &mut BlankNodes<'a>) -> usize {
    match blank_node {
        BlankNode::Anon => blank_nodes.new_blank(),
        BlankNode::BlankNode(label) => blank_nodes.get_blank(label),
    }
}

const RDF_FIRST: &'static str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &'static str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &'static str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

fn s2o(s: SingleSubject) -> SingleObject {
    match s {
        SingleSubject::IRI(iri) => SingleObject::IRI(iri),
        SingleSubject::BlankNode(n) => SingleObject::BlankNode(n),
    }
}

fn make_collection<'a>(collection: Vec<Object<'a>>,
                       blank_nodes: &mut BlankNodes<'a>,
                       triple_buffer: &mut Vec<Triple<'a>>)
                       -> Result<SingleSubject<'a>> {
    let mut head = SingleSubject::IRI(IRI::IRI(RDF_NIL));
    for object in collection.into_iter().rev() {
        let this = blank_nodes.new_blank();
        let o = make_object(object, blank_nodes, triple_buffer)?;
        triple_buffer.push(Triple {
            subject: SingleSubject::BlankNode(this),
            predicate: IRI::IRI(RDF_FIRST),
            object: o,
        });
        triple_buffer.push(Triple {
            subject: SingleSubject::BlankNode(this),
            predicate: IRI::IRI(RDF_REST),
            object: s2o(head),
        });
        head = SingleSubject::BlankNode(this);
    }
    Ok(head)
}

fn make_subject<'a>(subject: Subject<'a>,
                    blank_nodes: &mut BlankNodes<'a>,
                    triple_buffer: &mut Vec<Triple<'a>>)
                    -> Result<SingleSubject<'a>> {
    Ok(match subject {
        Subject::IRI(iri) => SingleSubject::IRI(iri),
        Subject::BlankNode(blank) => SingleSubject::BlankNode(make_blank(blank, blank_nodes)),
        Subject::Collection(collection) => make_collection(collection, blank_nodes, triple_buffer)?,
    })
}

fn make_object<'a>(object: Object<'a>,
                   blank_nodes: &mut BlankNodes<'a>,
                   triple_buffer: &mut Vec<Triple<'a>>)
                   -> Result<SingleObject<'a>> {
    Ok(match object {
        Object::IRI(iri) => SingleObject::IRI(iri),
        Object::BlankNode(blank) => SingleObject::BlankNode(make_blank(blank, blank_nodes)),
        Object::Collection(collection) => {
            s2o(make_collection(collection, blank_nodes, triple_buffer)?)
        }
        Object::Literal(l) => SingleObject::Literal(l),
        Object::BlankNodePropertyList(predicated_objects_list) => {
            let blank = blank_nodes.new_blank();
            let subject = SingleSubject::BlankNode(blank);
            add_predicated_objects(subject, predicated_objects_list, blank_nodes, triple_buffer)?;
            SingleObject::BlankNode(blank)
        }
    })
}

fn add_predicated_objects<'a>(subject: SingleSubject<'a>,
                              predicated_objects_list: Vec<PredicatedObjects<'a>>,
                              blank_nodes: &mut BlankNodes<'a>,
                              triple_buffer: &mut Vec<Triple<'a>>)
                              -> Result<()> {
    for po in predicated_objects_list {
        for o in po.objects.into_iter() {
            let triple = Triple {
                subject: subject.clone(),
                predicate: po.verb.clone(),
                object: make_object(o, blank_nodes, triple_buffer)?,
            };
            triple_buffer.push(triple);
        }
    }
    Ok(())
}

fn add_triples<'a>(new_triples: Triples<'a>,
                   blank_nodes: &mut BlankNodes<'a>,
                   triple_buffer: &mut Vec<Triple<'a>>)
                   -> Result<()> {
    let subject = make_subject(new_triples.subject, blank_nodes, triple_buffer)?;
    add_predicated_objects(subject,
                           new_triples.predicated_objects_list,
                           blank_nodes,
                           triple_buffer)
}

#[test]
fn blank_node() {
    let s = "<http://a.example/s> <http://a.example/p> _:b1 .\n";
    let mut i = StatementIterator::new(s).unwrap();
    let n = i.next();
    assert!(n.is_some());
    assert!(n.unwrap().is_ok());
}

#[test]
fn test_string_literal_long_quote() {
    let s = "<http://a.example/s> <http://a.example/p> \"\"\"first long literal\"\"\" .\n";
    let mut i = StatementIterator::new(s).unwrap();
    let n = i.next();
    assert!(n.is_some());
    assert!(n.unwrap().is_ok());
}

#[test]
fn test_no_space_before_dot() {
    let s = "@prefix : <urn:> .\n:s..2 :p..2 :o.\n";
    let mut i = StatementIterator::new(s).unwrap();
    i.next();
    let n = i.next();
    println!("{:?}", n);
    assert!(n.is_some());
    assert!(n.unwrap().is_ok());
}
