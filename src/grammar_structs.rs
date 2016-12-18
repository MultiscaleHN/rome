
#[derive(Debug,PartialEq,Eq,Clone)]
pub enum IRI {
    IRI(String),
    PrefixedName(String, String),
}

#[derive(Debug,PartialEq,Eq)]
pub enum RDFLiteralType {
    LangTag(String),
    DataType(IRI),
}

#[derive(Debug,PartialEq,Clone)]
pub enum Literal {
    LangString(String, String),
    XsdString(String),
    XsdInteger(i64),
    XsdDecimal(f64),
    XsdDouble(f64),
    XsdBoolean(bool),
    TypedLiteral(String, IRI),
}

#[derive(Debug,PartialEq)]
pub enum Subject {
    IRI(IRI),
    Anon,
    BlankNode(String),
}

#[derive(Debug,PartialEq,Clone)]
pub enum Object {
    IRI(IRI),
    Literal(Literal),
    BlankNodePropertyList(Vec<PredicatedObjects>),
}

#[derive(Debug,PartialEq,Clone)]
pub struct PredicatedObjects {
    pub verb: IRI,
    pub objects: Vec<Object>,
}

#[derive(Debug,PartialEq)]
pub struct Triples {
    pub subject: Subject,
    pub predicated_objects_list: Vec<PredicatedObjects>,
}

#[derive(Debug,PartialEq)]
pub enum Statement {
    Prefix(String, String),
    Base(String),
    Triples(Triples),
}
