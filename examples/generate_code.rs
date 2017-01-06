/// Generate rust code from a set of ontologies
///

extern crate rdfio;
use std::env::args;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::collections::BTreeMap;
use std::collections::HashSet;
use rdfio::graph_writer;
use rdfio::graph::{Object, Graph, GraphCreator, Triple, SubjectPtr, ObjectPtr};
use rdfio::triple_stream::*;
use rdfio::triple128::*;
use rdfio::namespaces::Namespaces;
use rdfio::resource::ResourceBase;
use rdfio::ontology::classes::rdf::Property;
use rdfio::ontology::classes::rdfs::Class;
use rdfio::ontology::properties::rdfs::{Comment, Domain, Range, SubClassOf};
use rdfio::ontology;
use rdfio::ontology_adapter;

type MyGraph = graph_writer::Graph<Triple128SPO, Triple128OPS>;
type OA = ontology_adapter::OntologyAdapter<MyGraph>;
type Writers = BTreeMap<Vec<u8>, Vec<u8>>;

macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

struct Output {
    mod_name: String,
    output_dir: PathBuf,
    internal: bool,
}

struct OntoData<G>
    where G: Graph
{
    o: Output,
    classes: Vec<Class<G>>,
    properties: Vec<Property<G>>,
    prefixes: Namespaces,
}

fn read_file(path: &str) -> io::Result<String> {
    let mut f = match fs::File::open(path) {
        Err(e) => {
            println_stderr!("Cannot open file {}.", path);
            return Err(e);
        }
        Ok(f) => f,
    };
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

fn camel_case(str: &str) -> String {
    let mut s = str[0..1].to_uppercase();
    s.push_str(&str[1..]);
    s.replace("-", "_")
}

fn snake_case(str: &str) -> String {
    if str == "type" {
        return String::from("a");
    }
    let mut s = str[0..1].to_lowercase();
    for c in str.chars().skip(1) {
        if c.is_uppercase() {
            s.push('_');
            for l in c.to_lowercase() {
                s.push(l);
            }
        } else {
            s.push(c);
        }
    }
    s.replace("-", "_")
}

fn comment_escape(str: &str) -> String {
    str.replace("\n", "")
}

fn write_impl_property<G, W>(class: &Class<G>,
                             property: &Property<G>,
                             mod_name: &str,
                             prefixes: &Namespaces,
                             done: &mut HashSet<String>,
                             writer: &mut W)
                             -> rdfio::Result<()>
    where W: Write,
          G: Graph
{
    if let Some(iri) = property.this().iri() {
        if done.contains(iri) {
            return Ok(());
        }
        if let Some((prop_prefix, prop)) = prefixes.find_prefix(iri) {
            if let Some(domain) = class.this().iri() {
                if let Some((_, domain)) = prefixes.find_prefix(domain) {
                    writer.write_all(
                        format!("impl<G> {}::properties::{}::{}<G> for {}<G> where G: graph::Graph {{}}\n",
                            mod_name,
                            String::from_utf8_lossy(prop_prefix),
                            camel_case(prop),
                            camel_case(domain)).as_bytes())?;
                    done.insert(String::from(iri));
                }
            }
        }
    }
    Ok(())
}

fn write_impl_properties<G, W>(class: &Class<G>,
                               parent: &Class<G>,
                               d: &OntoData<G>,
                               done: &mut HashSet<String>,
                               writer: &mut W)
                               -> rdfio::Result<()>
    where W: Write,
          G: Graph
{
    for property in d.properties.iter() {
        for domain in property.domain() {
            if domain == *parent {
                write_impl_property(class, property, &d.o.mod_name, &d.prefixes, done, writer)?;
            }
        }
    }
    for parent in parent.sub_class_of() {
        write_impl_properties(class, &parent, d, done, writer)?;
    }
    Ok(())
}

const RDFS_SUB_CLASS_OF: &'static str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_CLASS: &'static str = "http://www.w3.org/2000/01/rdf-schema#Class";
const RDF_TYPE: &'static str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn infer(graph: &MyGraph) -> rdfio::Result<MyGraph> {
    // for every triple with rdfs:subClassOf infer that the subject and the
    // object are rdfs:Class instances
    let mut writer = graph_writer::GraphWriter::with_capacity(65000);
    for triple in graph.iter().filter(|triple| triple.predicate() == RDFS_SUB_CLASS_OF) {
        writer.add(triple.subject(), RDF_TYPE, RDFS_CLASS);
        match triple.object() {
            Object::IRI(iri) => {
                writer.add(iri, RDF_TYPE, RDFS_CLASS);
            }
            Object::BlankNode(b) => {
                writer.add(b, RDF_TYPE, RDFS_CLASS);
            }
            _ => {}
        }
    }
    for triple in graph.iter() {
        writer.add_triple(&triple);
    }
    Ok(writer.collect().sort_blank_nodes())
}

fn write_mod(o: &Output, iris: &Vec<String>) -> rdfio::Result<()> {
    let path = o.output_dir.join("mod.rs");
    let mut mod_rs = fs::File::create(path)?;
    mod_rs.write_all(b"pub mod classes;\n")?;
    mod_rs.write_all(b"pub mod properties;\n")?;
    mod_rs.write_all(b"use std;\n")?;
    if o.internal {
        mod_rs.write_all(b"use graph;\n")?;
        mod_rs.write_all(b"use ontology_adapter;\n")?;
    } else {
        mod_rs.write_all(b"use rdfio::graph;\n")?;
        mod_rs.write_all(b"use rdfio::ontology_adapter;\n")?;
    }
    mod_rs.write_all(b"pub fn adapter<G>(graph: &std::rc::Rc<G>) -> ontology_adapter::OntologyAdapter<G>
    where G: graph::Graph
{
    let mut iris = Vec::with_capacity(")?;
    mod_rs.write_all(format!("{});\n", iris.len()).as_bytes())?;
    for iri in iris {
        mod_rs.write_all(format!("    iris.push(graph.predicate_ptr(\"{}\"));\n", iri).as_bytes())?;
    }
    mod_rs.write_all(b"    ontology_adapter::OntologyAdapter::new(graph, iris)\n}\n")?;
    Ok(())
}

fn load_files(inputs: &Vec<String>) -> rdfio::Result<(Namespaces, Rc<MyGraph>)> {
    let mut writer = graph_writer::GraphWriter::with_capacity(65000);
    let mut prefixes = Namespaces::new();
    for input in inputs {
        let data = read_file(input)?;
        let mut base = String::from("file:");
        base.push_str(input);
        let mut triples = TripleIterator::new(data.as_str(), &base)?;
        while let Some(triple) = triples.next() {
            writer.add_triple(&triple?);
        }
        for ns in triples.prefixes().iter() {
            prefixes.set(ns.prefix(), ns.namespace());
        }
    }
    let graph = writer.collect();
    let graph = infer(&graph)?;
    Ok((prefixes, Rc::new(graph)))
}

fn generate_classes<G>(d: &OntoData<G>, iris: &mut Vec<String>) -> rdfio::Result<()>
    where G: Graph
{
    let mut outputs = BTreeMap::new();
    for ns in d.prefixes.iter() {
        outputs.insert(Vec::from(ns.prefix()), Vec::new());
    }
    for class in d.classes.iter() {
        if let Some(iri) = class.this().iri() {
            if let Some((prefix, name)) = d.prefixes.find_prefix(iri) {
                if let Some(mut writer) = outputs.get_mut(prefix) {
                    writer.write_all(b"\n/// ")?;
                    writer.write_all(prefix)?;
                    writer.write_all(b":")?;
                    writer.write_all(name.as_bytes())?;
                    for comment in class.comment() {
                        if let Some(l) = comment.this().literal() {
                            writer.write_all(b"\n/// ")?;
                            writer.write_all(comment_escape(l).as_bytes())?;
                        }
                    }
                    writer.write_all(format!("\nclass!(\"{}\", {}, {});\n", iri,
                            camel_case(name), iris.len())
                            .as_bytes())?;
                    let mut done = HashSet::new();
                    write_impl_properties(&class, &class, d, &mut done, &mut writer)?;
                    iris.push(String::from(iri));
                }
            }
        }
    }
    write_files(&d.o, &outputs, "classes", true)
}

fn generate_properties<G>(d: &OntoData<G>, iris: &mut Vec<String>) -> rdfio::Result<()>
    where G: Graph
{
    let mut outputs = BTreeMap::new();
    for ns in d.prefixes.iter() {
        outputs.insert(Vec::from(ns.prefix()), Vec::new());
    }
    for property in d.properties.iter() {
        if let Some(iri) = property.this().iri() {
            if let Some((prop_prefix, prop)) = d.prefixes.find_prefix(iri) {
                for range in property.range() {
                    if let Some((prefix, range)) =
                        range.this().iri().and_then(|i| d.prefixes.find_prefix(i)) {
                        if let Some(mut writer) = outputs.get_mut(prop_prefix) {
                            writer.write_all(b"\n/// ")?;
                            writer.write_all(prop_prefix)?;
                            writer.write_all(b":")?;
                            writer.write_all(prop.as_bytes())?;
                            for comment in property.comment() {
                                if let Some(l) = comment.this().literal() {
                                    writer.write_all(b"\n/// ")?;
                                    writer.write_all(comment_escape(l).as_bytes())?;
                                }
                            }
                            writer.write_all(
                                    format!("\nproperty!(\"{}\", {}, {}, {}::classes::{}::{}<G>, {});\n",
                                        iri, camel_case(prop),
                                        snake_case(prop), d.o.mod_name,
                                        String::from_utf8_lossy(prefix),
                                        camel_case(range), iris.len()).as_bytes())?;
                        }
                    }
                }
                iris.push(String::from(iri));
            }
        }
    }
    write_files(&d.o, &outputs, "properties", false)
}

fn uses(o: &Output, classes: bool) -> String {
    let mut uses = String::new();
    uses.push_str("use std;\n");
    if o.internal {
        uses.push_str("use graph;\n");
        uses.push_str("use resource;\n");
        if classes {
            uses.push_str("use ontology_adapter;\n");
        }
    } else {
        uses.push_str("use rdfio::graph;\n");
        uses.push_str("use rdfio::resource;\n");
        if classes {
            uses.push_str("use rdfio::ontology_adapter;\n");
        }
    }
    uses.push_str(&format!("use {};\n", o.mod_name));
    uses
}

fn write_files(o: &Output, writers: &Writers, folder: &str, classes: bool) -> rdfio::Result<()> {
    let uses = uses(o, classes);
    let dir_path = o.output_dir.join(folder);
    if !fs::metadata(&dir_path)?.is_dir() {
        fs::create_dir(&dir_path)?;
    }
    let path = dir_path.join("mod.rs");
    let mut mod_rs = fs::File::create(path)?;
    for (prefix, content) in writers.iter() {
        if content.len() > 0 {
            let mut filename = String::from_utf8_lossy(prefix).into_owned();
            filename.push_str(".rs");
            let path = dir_path.join(filename);
            let mut file = fs::File::create(path)?;
            file.write_all(uses.as_bytes())?;
            file.write_all(content)?;
            mod_rs.write_all(b"pub mod ")?;
            mod_rs.write_all(prefix)?;
            mod_rs.write_all(b";\n")?;
        }
    }
    Ok(())
}

fn generate(output_dir: PathBuf,
            mod_name: String,
            internal: bool,
            inputs: &Vec<String>)
            -> rdfio::Result<()> {
    let (prefixes, graph) = load_files(inputs)?;
    let oa = Rc::new(ontology::adapter(&graph));

    let data = OntoData {
        o: Output {
            mod_name: mod_name,
            output_dir: output_dir,
            internal: internal,
        },
        classes: Class::iter(&oa).collect(),
        properties: Property::iter(&oa).collect(),
        prefixes: prefixes,
    };

    let mut iris = Vec::new();
    // rdf:type is always needed
    iris.push(String::from(RDF_TYPE));
    generate_classes(&data, &mut iris)?;
    generate_properties(&data, &mut iris)?;
    write_mod(&data.o, &iris)?;
    Ok(())
}

fn main() {
    let mut args = args();
    let exe = args.next().unwrap();
    if args.len() < 2 {
        println_stderr!("Usage: {} [--mod MOD] OUTPUT_DIR INPUT_FILES", exe);
        std::process::exit(-1);
    }
    let arg = args.next().unwrap();
    let internal; // is the command run for rdfio itself?
    let mod_name;
    let output_dir;
    if args.len() > 2 && arg == "--mod" {
        mod_name = args.next().unwrap();
        internal = false;
        output_dir = args.next().unwrap();
    } else {
        mod_name = String::from("ontology");
        internal = true;
        output_dir = arg;
    }
    let inputs = args.collect();
    if let Err(e) = generate(PathBuf::from(output_dir), mod_name, internal, &inputs) {
        println_stderr!("ERROR {:?}", e);
        std::process::exit(-1);
    }
}
