use std::mem;
use std::slice;
use std::rc::Rc;
use grammar;
use graph;
use string_collector::*;
use std::fmt::Debug;
#[cfg(test)]
use triple64::*;
use compact_triple::*;
use std::cmp;
use std::marker::PhantomData;

pub struct GraphWriter<SPO, OPS>
    where SPO: CompactTriple<u32>,
          OPS: CompactTriple<u32>
{
    string_collector: StringCollector,
    datatype_lang_collector: StringCollector,
    triples: Vec<SPO>,
    prev_subject_iri: Option<(String, StringId)>,
    prev_predicate: Option<(String, StringId)>,
    prev_datatype: Option<(String, StringId)>,
    prev_lang: Option<(String, StringId)>,
    highest_blank_node: u32,
    phantom: PhantomData<OPS>,
}
pub struct Graph<SPO, OPS>
    where SPO: CompactTriple<u32>,
          OPS: CompactTriple<u32>
{
    strings: Rc<StringCollection>,
    datatype_or_lang: Rc<StringCollection>,
    spo: Vec<SPO>,
    ops: Vec<OPS>,
    highest_blank_node: u32,
}

fn translate<T>(t: &mut T, translation: &Vec<StringId>, datatrans: &Vec<StringId>)
    where T: CompactTriple<u32>
{
    if t.subject_is_iri() {
        let subject = t.subject() as usize;
        t.set_subject(translation[subject].id);
    }
    let predicate = t.predicate() as usize;
    t.set_predicate(translation[predicate].id);
    if !t.object_is_blank_node() {
        let object = t.object() as usize;
        t.set_object(translation[object].id);
        if !t.object_is_iri() {
            let datatype_or_lang = t.datatype_or_lang() as usize;
            t.set_datatype_or_lang(datatrans[datatype_or_lang].id);
        }
    }
}
fn translate_object<T>(t: &mut T, translation: &Vec<u32>)
    where T: CompactTriple<u32>
{
    if !t.subject_is_iri() {
        let subject = t.subject() as usize;
        t.set_subject(translation[subject]);
    }
    if t.object_is_blank_node() {
        let object = t.object() as usize;
        t.set_object(translation[object]);
    }
}
/// check if the new string is the same as the string from the previous triple
/// if the string is the same, use the re-use the id
fn check_prev(string: &str,
              prev: &mut Option<(String, StringId)>,
              string_collector: &mut StringCollector)
              -> StringId {
    let id;
    if let Some((mut prev_string, prev_id)) = prev.take() {
        if string == prev_string {
            id = prev_id;
        } else {
            id = string_collector.add_string(string);
            prev_string.clear();
            prev_string.push_str(string);
        }
        *prev = Some((prev_string, id));
    } else {
        id = string_collector.add_string(string);
        *prev = Some((String::from(string), id));
    }
    id
}
impl<SPO, OPS> GraphWriter<SPO, OPS>
    where SPO: CompactTriple<u32>,
          OPS: CompactTriple<u32>
{
    pub fn with_capacity(capacity: usize) -> GraphWriter<SPO, OPS> {
        GraphWriter {
            string_collector: StringCollector::with_capacity(capacity),
            datatype_lang_collector: StringCollector::with_capacity(capacity),
            triples: Vec::new(),
            prev_subject_iri: None,
            prev_predicate: None,
            prev_datatype: None,
            prev_lang: None,
            highest_blank_node: 0,
            phantom: PhantomData,
        }
    }
    pub fn highest_blank_node(&self) -> u32 {
        self.highest_blank_node
    }
    fn add_s_iri(&mut self, s: &str, p: &str, ot: TripleObjectType, o: u32, d: u32) {
        let s = check_prev(s, &mut self.prev_subject_iri, &mut self.string_collector);
        let p = check_prev(p, &mut self.prev_predicate, &mut self.string_collector);
        let t = SPO::triple(true, s.id, p.id, ot, o, d);
        self.triples.push(t);
    }
    pub fn add_iri_blank(&mut self, subject: &str, predicate: &str, object: u32) {
        self.highest_blank_node = cmp::max(self.highest_blank_node, object);
        self.add_s_iri(subject, predicate, TripleObjectType::BlankNode, object, 0);
    }
    pub fn add_iri_iri(&mut self, subject: &str, predicate: &str, object: &str) {
        let o = self.string_collector.add_string(object);
        self.add_s_iri(subject, predicate, TripleObjectType::IRI, o.id, 0);
    }
    pub fn add_iri_lit(&mut self, subject: &str, predicate: &str, object: &str, datatype: &str) {
        let o = self.string_collector.add_string(object);
        let d = check_prev(datatype,
                           &mut self.prev_datatype,
                           &mut self.datatype_lang_collector)
            .id;
        self.add_s_iri(subject, predicate, TripleObjectType::Literal, o.id, d);
    }
    pub fn add_iri_lit_lang(&mut self, subject: &str, predicate: &str, object: &str, lang: &str) {
        let o = self.string_collector.add_string(object);
        let l = check_prev(lang, &mut self.prev_lang, &mut self.datatype_lang_collector).id;
        self.add_s_iri(subject, predicate, TripleObjectType::LiteralLang, o.id, l);
    }
    fn add_s_blank(&mut self, s: u32, p: &str, ot: TripleObjectType, o: u32, d: u32) {
        self.highest_blank_node = cmp::max(self.highest_blank_node, s);
        let p = check_prev(p, &mut self.prev_predicate, &mut self.string_collector);
        let t = SPO::triple(false, s, p.id, ot, o, d);
        self.triples.push(t);
    }
    pub fn add_blank_blank(&mut self, subject: u32, predicate: &str, object: u32) {
        self.highest_blank_node = cmp::max(self.highest_blank_node, object);
        self.add_s_blank(subject, predicate, TripleObjectType::BlankNode, object, 0);
    }
    pub fn add_blank_iri(&mut self, subject: u32, predicate: &str, object: &str) {
        let o = self.string_collector.add_string(object);
        self.add_s_blank(subject, predicate, TripleObjectType::IRI, o.id, 0);
    }
    pub fn add_blank_lit(&mut self, subject: u32, predicate: &str, object: &str, datatype: &str) {
        let o = self.string_collector.add_string(object);
        let d = check_prev(datatype,
                           &mut self.prev_datatype,
                           &mut self.datatype_lang_collector)
            .id;
        self.add_s_blank(subject, predicate, TripleObjectType::Literal, o.id, d);
    }
    pub fn add_blank_lit_lang(&mut self, subject: u32, predicate: &str, object: &str, lang: &str) {
        let o = self.string_collector.add_string(object);
        let l = check_prev(lang, &mut self.prev_lang, &mut self.datatype_lang_collector).id;
        self.add_s_blank(subject, predicate, TripleObjectType::LiteralLang, o.id, l);
    }
    fn add_(&mut self, subject: graph::Subject, predicate: &str, object: graph::Object) {
        match subject {
            graph::Subject::IRI(subject) => {
                match object {
                    graph::Object::IRI(object) => {
                        self.add_iri_iri(subject, predicate, object);
                    }
                    graph::Object::BlankNode(object) => {
                        self.add_iri_blank(subject, predicate, object.0 as u32);
                    }
                    graph::Object::Literal(object) => {
                        match object.language {
                            None => {
                                self.add_iri_lit(subject,
                                                 predicate,
                                                 object.lexical,
                                                 object.datatype);
                            }
                            Some(lang) => {
                                self.add_iri_lit_lang(subject, predicate, object.lexical, lang);
                            }
                        }
                    }
                }
            }
            graph::Subject::BlankNode(subject) => {
                match object {
                    graph::Object::IRI(object) => {
                        self.add_blank_iri(subject.0 as u32, predicate, object);
                    }
                    graph::Object::BlankNode(object) => {
                        self.add_blank_blank(subject.0 as u32, predicate, object.0 as u32);
                    }
                    graph::Object::Literal(object) => {
                        match object.language {
                            None => {
                                self.add_blank_lit(subject.0 as u32,
                                                   predicate,
                                                   object.lexical,
                                                   object.datatype);
                            }
                            Some(lang) => {
                                self.add_blank_lit_lang(subject.0 as u32,
                                                        predicate,
                                                        object.lexical,
                                                        lang);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn create_ops<SPO, OPS>(spo: &[SPO]) -> Vec<OPS>
    where SPO: CompactTriple<u32>,
          OPS: CompactTriple<u32> + Ord
{
    let mut ops = Vec::with_capacity(spo.len());
    for t in spo {
        ops.push(OPS::triple(t.subject_is_iri(),
                             t.subject(),
                             t.predicate(),
                             t.object_type(),
                             t.object(),
                             t.datatype_or_lang()));
    }
    ops.sort();
    ops
}

impl<SPO, OPS> graph::GraphCreator for GraphWriter<SPO, OPS>
    where SPO: CompactTriple<u32> + Ord + Copy,
          OPS: CompactTriple<u32> + Ord
{
    type Graph = Graph<SPO, OPS>;
    fn add_triple<T>(&mut self, triple: &T)
        where T: graph::Triple
    {
        self.add_(triple.subject(), triple.predicate(), triple.object());
    }

    fn collect(&mut self) -> Graph<SPO, OPS> {
        let (translation, string_collection) = self.string_collector.collect();
        let (datatrans, datatype_lang_collection) = self.datatype_lang_collector.collect();
        let mut spo = Vec::new();
        mem::swap(&mut spo, &mut self.triples);
        for t in spo.iter_mut() {
            translate(t, &translation, &datatrans);
        }
        // sort according to StringId, which is sorted alphabetically
        spo.sort();
        spo.dedup();
        spo.shrink_to_fit();
        let ops = create_ops(&spo);
        Graph {
            strings: Rc::new(string_collection),
            datatype_or_lang: Rc::new(datatype_lang_collection),
            spo: spo,
            ops: ops,
            highest_blank_node: self.highest_blank_node,
        }
    }
    fn create_blank_node(&mut self) -> graph::BlankNode {
        self.highest_blank_node += 1;
        (self.highest_blank_node as usize, 0)
    }
    fn add<'b, S, O>(&mut self, subject: S, predicate: &str, object: O)
        where S: graph::IntoSubject<'b>,
              O: graph::IntoObject<'b>
    {
        self.add_(subject.subject(), predicate, object.object());
    }
}

pub struct GraphTriple<T>
    where T: PartialEq
{
    strings: Rc<StringCollection>,
    datatype_or_lang: Rc<StringCollection>,
    triple: T,
}

impl<T> PartialEq for GraphTriple<T>
    where T: PartialEq + CompactTriple<u32>
{
    fn eq(&self, other: &Self) -> bool {
        // if the triples use the same StringCollection, it's ok to compare
        // the numeric value of the triple
        if Rc::ptr_eq(&self.strings, &other.strings) {
            self.triple.eq(&other.triple)
        } else {
            use graph::Triple;
            self.subject().eq(&other.subject()) && self.predicate().eq(other.predicate()) &&
            self.object().eq(&other.object())
        }
    }
}

struct GraphIterator<'a, T: 'a> {
    strings: Rc<StringCollection>,
    datatype_or_lang: Rc<StringCollection>,
    iter: slice::Iter<'a, T>,
}

impl<'a, T> Iterator for GraphIterator<'a, T>
    where T: Copy + PartialEq
{
    type Item = GraphTriple<T>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|t| {
            GraphTriple {
                strings: self.strings.clone(),
                datatype_or_lang: self.datatype_or_lang.clone(),
                triple: *t,
            }
        })
    }
}

impl<T> graph::Triple for GraphTriple<T>
    where T: CompactTriple<u32> + PartialEq
{
    fn subject(&self) -> graph::Subject {
        if self.triple.subject_is_iri() {
            graph::Subject::IRI(self.strings.get(StringId { id: self.triple.subject() }))
        } else {
            graph::Subject::BlankNode((self.triple.subject() as usize, 0))
        }
    }
    fn predicate(&self) -> &str {
        self.strings.get(StringId { id: self.triple.predicate() })
    }
    fn object(&self) -> graph::Object {
        if self.triple.object_is_iri() {
            graph::Object::IRI(self.strings.get(StringId { id: self.triple.object() }))
        } else if self.triple.object_is_blank_node() {
            graph::Object::BlankNode((self.triple.object() as usize, 0))
        } else if self.triple.has_language() {
            graph::Object::Literal(graph::Literal {
                lexical: self.strings.get(StringId { id: self.triple.object() }),
                datatype: grammar::RDF_LANG_STRING,
                language: Some(self.datatype_or_lang
                    .get(StringId { id: self.triple.datatype_or_lang() })),
            })
        } else {
            graph::Object::Literal(graph::Literal {
                lexical: self.strings.get(StringId { id: self.triple.object() }),
                datatype: self.datatype_or_lang
                    .get(StringId { id: self.triple.datatype_or_lang() }),
                language: None,
            })
        }
    }
}

pub struct TripleRangeIterator<'a, T: 'a>
    where T: CompactTriple<u32>
{
    strings: Rc<StringCollection>,
    datatype_or_lang: Rc<StringCollection>,
    iter: slice::Iter<'a, T>,
    end: T,
}

impl<'a, T> Iterator for TripleRangeIterator<'a, T>
    where T: Ord + CompactTriple<u32> + Copy + Debug
{
    type Item = GraphTriple<T>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(t) if *t < self.end => {
                Some(GraphTriple {
                    strings: self.strings.clone(),
                    datatype_or_lang: self.datatype_or_lang.clone(),
                    triple: *t,
                })
            }
            _ => None,
        }
    }
}

fn subject_blank_node<SPO>(subject: u32) -> SPO
    where SPO: CompactTriple<u32>
{
    SPO::triple(false, subject, 0, TripleObjectType::BlankNode, 0, 0)
}
fn subject_iri<SPO>(subject: u32) -> SPO
    where SPO: CompactTriple<u32>
{
    SPO::triple(true, subject, 0, TripleObjectType::BlankNode, 0, 0)
}
fn object_blank_node<OPS>(object: u32) -> OPS
    where OPS: CompactTriple<u32>
{
    OPS::triple(false, 0, 0, TripleObjectType::BlankNode, object, 0)
}
fn object_iri<OPS>(object: u32) -> OPS
    where OPS: CompactTriple<u32>
{
    OPS::triple(false, 0, 0, TripleObjectType::IRI, object, 0)
}
fn object_iri_predicate<OPS>(object: u32, predicate: u32) -> OPS
    where OPS: CompactTriple<u32>
{
    OPS::triple(true, 0, predicate, TripleObjectType::IRI, object, 0)
}

impl<SPO, OPS> Graph<SPO, OPS>
    where SPO: CompactTriple<u32> + Copy + Ord + Debug,
          OPS: CompactTriple<u32> + Copy + Ord + Debug
{
    fn range_iter<'a, T>(&self, index: &'a [T], start: T, end: T) -> TripleRangeIterator<'a, T>
        where T: CompactTriple<u32> + Ord + Copy
    {
        let slice = match index.binary_search(&start) {
            Ok(pos) => &index[pos..],
            Err(pos) => &index[pos..],
        };
        TripleRangeIterator {
            strings: self.strings.clone(),
            datatype_or_lang: self.datatype_or_lang.clone(),
            iter: slice.iter(),
            end: end,
        }
    }
    fn empty_range_iter<T>(&self) -> TripleRangeIterator<T>
        where T: CompactTriple<u32> + Ord
    {
        let end = T::triple(true, 0, 0, TripleObjectType::BlankNode, 0, 0);
        TripleRangeIterator {
            strings: self.strings.clone(),
            datatype_or_lang: self.datatype_or_lang.clone(),
            iter: [].iter(),
            end: end,
        }
    }
    /// iterator over all triples with the same subject
    fn iter_subject_(&self, triple: SPO) -> TripleRangeIterator<SPO> {
        let mut end = triple;
        end.set_subject(triple.subject() + 1);
        self.range_iter(&self.spo, triple, end)
    }
    pub fn iter_subject(&self, subject: &graph::Subject) -> TripleRangeIterator<SPO> {
        match *subject {
            graph::Subject::IRI(iri) => self.iter_subject_iri(iri),
            graph::Subject::BlankNode(_) => self.empty_range_iter(),
        }
    }
    /// iterator over all triples with the same subject
    pub fn iter_subject_iri(&self, iri: &str) -> TripleRangeIterator<SPO> {
        match self.strings.find(iri) {
            None => self.empty_range_iter(),
            Some(id) => {
                let triple = subject_iri(id.id);
                self.iter_subject_(triple)
            }
        }
    }
    /// iterator over all triples with the same object
    fn iter_object(&self, triple: OPS) -> TripleRangeIterator<OPS> {
        let mut end = triple;
        end.set_object(triple.object() + 1);
        self.range_iter(&self.ops, triple, end)
    }
    /// iterator over all triples with the same object
    pub fn iter_object_iri(&self, iri: &str) -> TripleRangeIterator<OPS> {
        match self.strings.find(iri) {
            None => self.empty_range_iter(),
            Some(id) => {
                let triple = object_iri(id.id);
                self.iter_object(triple)
            }
        }
    }
    /// iterator over all triples with the same object and predicate
    fn iter_object_predicate(&self, triple: OPS) -> TripleRangeIterator<OPS> {
        let mut end = triple;
        end.set_predicate(triple.predicate() + 1);
        self.range_iter(&self.ops, triple, end)
    }
    /// iterator over all triples with the same object and predicate
    pub fn iter_object_iri_predicate(&self,
                                     object_iri: &str,
                                     predicate: &str)
                                     -> TripleRangeIterator<OPS> {
        match self.strings.find(object_iri) {
            None => self.empty_range_iter(),
            Some(object) => {
                match self.strings.find(predicate) {
                    None => self.empty_range_iter(),
                    Some(predicate) => {
                        let triple = object_iri_predicate(object.id, predicate.id);
                        self.iter_object_predicate(triple)
                    }
                }
            }
        }
    }
    /// iterate over all triple with a blank node subject
    pub fn iter_subject_blank_nodes(&self) -> TripleRangeIterator<SPO> {
        let start = subject_blank_node(0);
        let end = subject_iri(0);
        self.range_iter(&self.spo, start, end)
    }
    /// iterate over all triple with a blank node object
    pub fn iter_object_blank_nodes(&self) -> TripleRangeIterator<OPS> {
        let start = object_blank_node(0);
        let end = object_iri(0);
        self.range_iter(&self.ops, start, end)
    }
    pub fn sort_blank_nodes(&self) -> Graph<SPO, OPS> {
        // sort nodes by usage (least used last)
        self.sort_blank_nodes_by(|b1, b2| {
            let mut cmp = b2.times_a_subject.cmp(&b1.times_a_subject);
            if cmp == cmp::Ordering::Equal {
                cmp = b2.times_a_subject_with_blank_object
                    .cmp(&b1.times_a_subject_with_blank_object);
            }
            if cmp == cmp::Ordering::Equal {
                cmp = b2.times_an_object.cmp(&b1.times_an_object);
            }
            if cmp == cmp::Ordering::Equal {
                cmp = b2.times_an_object_with_blank_subject
                    .cmp(&b1.times_an_object_with_blank_subject);
            }
            // if usage is equal compare the triples that the nodes are in
            if cmp == cmp::Ordering::Equal {
                let s1 = self.iter_subject_(subject_blank_node(b1.blank_node));
                let s2 = self.iter_subject_(subject_blank_node(b2.blank_node));
                cmp = s1.zip(s2)
                    .map(|(a, b)| compare_without_blank_nodes(a.triple, b.triple))
                    .find(|cmp| *cmp != cmp::Ordering::Equal)
                    .unwrap_or(cmp::Ordering::Equal);
            }
            if cmp == cmp::Ordering::Equal {
                let o1 = self.iter_object(object_blank_node(b1.blank_node));
                let o2 = self.iter_object(object_blank_node(b2.blank_node));
                cmp = o1.zip(o2)
                    .map(|(a, b)| compare_without_blank_nodes(a.triple, b.triple))
                    .find(|cmp| *cmp != cmp::Ordering::Equal)
                    .unwrap_or(cmp::Ordering::Equal);
            }
            cmp
        })
    }
    fn sort_blank_nodes_by<F>(&self, compare: F) -> Graph<SPO, OPS>
        where F: FnMut(&BlankNodeInfo, &BlankNodeInfo) -> cmp::Ordering
    {
        let len = self.highest_blank_node as usize + 1;
        let mut blank_info = Vec::with_capacity(len);
        for i in 0..len {
            blank_info.push(BlankNodeInfo {
                blank_node: i as u32,
                times_a_subject: 0,
                times_a_subject_with_blank_object: 0,
                times_an_object: 0,
                times_an_object_with_blank_subject: 0,
            })
        }
        // collection information on the blank nodes
        for t in self.iter_subject_blank_nodes() {
            let i = &mut blank_info[t.triple.subject() as usize];
            i.times_a_subject += 1;
            if t.triple.object_is_blank_node() {
                i.times_a_subject_with_blank_object += 1;
            }
        }
        for t in self.iter_object_blank_nodes() {
            let i = &mut blank_info[t.triple.object() as usize];
            i.times_an_object += 1;
            if !t.triple.subject_is_iri() {
                i.times_an_object_with_blank_subject += 1;
            }
        }
        // sort the vector
        blank_info.sort_by(compare);
        let mut translation = vec![0 as u32;len];
        for i in 0..len {
            translation[blank_info[i].blank_node as usize] = i as u32;
        }
        blank_info.clear();
        blank_info.shrink_to_fit();

        // translate the blank nodes in spo and ops
        let mut spo = self.spo.clone();
        for t in spo.iter_mut() {
            translate_object(t, &translation);
        }
        spo.sort();
        let mut ops = self.ops.clone();
        for t in ops.iter_mut() {
            translate_object(t, &translation);
        }
        ops.sort();

        Graph {
            strings: self.strings.clone(),
            datatype_or_lang: self.datatype_or_lang.clone(),
            spo: spo,
            ops: ops,
            highest_blank_node: self.highest_blank_node,
        }
    }
}

fn zero_blank_nodes<T>(a: &mut T)
    where T: CompactTriple<u32>
{
    if !a.subject_is_iri() {
        a.set_subject(0);
    }
    if a.object_is_blank_node() {
        a.set_subject(0);
    }
}

fn compare_without_blank_nodes<T>(mut a: T, mut b: T) -> cmp::Ordering
    where T: CompactTriple<u32> + Ord
{
    zero_blank_nodes(&mut a);
    zero_blank_nodes(&mut b);
    a.cmp(&b)
}

struct BlankNodeInfo {
    blank_node: u32,
    times_a_subject: u32,
    times_a_subject_with_blank_object: u32,
    times_an_object: u32,
    times_an_object_with_blank_subject: u32,
}

impl<SPO, OPS> graph::Graph for Graph<SPO, OPS>
    where SPO: CompactTriple<u32> + PartialEq + Copy,
          OPS: CompactTriple<u32>
{
    type Triple = GraphTriple<SPO>;
    fn iter<'a>(&'a self) -> Box<Iterator<Item = Self::Triple> + 'a> {
        Box::new(GraphIterator {
            strings: self.strings.clone(),
            datatype_or_lang: self.datatype_or_lang.clone(),
            iter: self.spo.iter(),
        })
    }
    fn len(&self) -> usize {
        self.spo.len()
    }
}

#[test]
fn collect_empty() {
    let mut writer: GraphWriter<Triple64SPO, Triple64OPS> = GraphWriter::with_capacity(0);
    use graph::GraphCreator;
    writer.collect();
}

#[test]
fn keep_blank_subject() {
    let mut writer: GraphWriter<Triple64SPO, Triple64OPS> = GraphWriter::with_capacity(0);
    writer.add_blank_blank(1, "", 2);
    use graph::{GraphCreator, Graph, Triple};
    let graph = writer.collect();
    let triple = graph.iter().next().unwrap();
    assert_eq!(triple.subject(), graph::Subject::BlankNode((1, 0)));
    assert_eq!(triple.predicate(), "");
    assert_eq!(triple.object(), graph::Object::BlankNode((2, 0)));
}
