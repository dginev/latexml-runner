#[macro_use]
extern crate clap;
extern crate csv;
extern crate which;

use std::error::Error;
use std::result::Result;

use latexml_runner::Harness;
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn Error>> {
  let mut matches = clap_app!(latexml_runner =>
        (version: "1.0")
        (author: "Deyan Ginev. <deyan.ginev@gmail.com>")
        (about: "A high-performance client for the latexmls daemonized socket server for LaTeXML")
        (@arg PORT: -p --from_port +takes_value "Sets the first port at which to deploy latexmls. Default is 3334.")
        (@arg INPUT: -i --input_file +takes_value +required "An input CSV file containing one formula per line. OR a directory of such CSV files.")
        (@arg OUTPUT: -o --output_file +takes_value +required "The output CSV file, containing one output formula per line, preserving input order. OR a directory for such CSV files.")
        (@arg LOG: -l --log_file +takes_value "An optional log file, containing one latexml conversion status per line, preserving input order")
        (@arg pmml: --pmml "converts math to Presentation MathML (default for xhtml & html5 formats)")
        (@arg nopmml: --nopmml "disable presentation MathML output")
        (@arg cmml: --cmml "enable content MathML output")
        (@arg nocmml: --nocmml "disable content MathML output")
        (@arg preload: --preload +takes_value ... "requests loading of an optional module can be repeated")
        (@arg preamble: --preamble +takes_value "loads a tex file containing document frontmatter. MUST include \\begin{document} or equivalent")
        (@arg postamble: --postamble +takes_value "loads a tex file containing document backmatter. MUST include \\end{document} or equivalent")
        (@arg includestyles: --includestyles     "allows latexml to load raw *.sty file; by default it avoids this.")
        (@arg base: --base +takes_value          "sets the current working directory")
        (@arg path: --path +takes_value ...      "adds dir to the paths searched for files, modules, etc;")
        (@arg log: --log +takes_value            "specifies log file (default: STDERR)")
        (@arg autoflush: --autoflush +takes_value  "Automatically restart the daemon after \"count\" inputs. Good practice for vast batch jobs. (default: 10000)")
        (@arg timeout: --timeout +takes_value    "Timecap for conversions (default 600)")
        (@arg expire: --expire +takes_value      "Timecap for server inactivity (default 600)")
        (@arg address: --address +takes_value    "Specify server address (default: localhost)")
        (@arg port: --port +takes_value          "Specify server port (default: 3354)")
        (@arg documentid: --documentid +takes_value    "assign an id to the document root.")
        (@arg quiet: --quiet                     "suppress messages (can repeat)")
        (@arg verbose: --verbose                 "more informative output (can repeat)")
        (@arg strict: --strict                   "makes latexml less forgiving of errors")
        (@arg bibtex: --bibtex                   "processes a BibTeX bibliography.")
        (@arg xml: --xml                         "requests xml output (default).")
        (@arg tex: --tex                         "requests TeX output after expansion.")
        (@arg format: --format +takes_value      "requests 'name' as the output format. Supported: tex,box,xml,html4,html5,xhtml html implies html5")
        (@arg noparse: --noparse conflicts_with[pmml cmml openmath]                 "suppresses parsing math (default: off)")
        (@arg parse: --parse +takes_value        "enables parsing math (default: on) and selects parser framework \"name\". Supported: RecDescent, no")
        (@arg profile: --profile +takes_value    "specify profile as defined in LaTeXML::Common::Config Supported: standard|math|fragment|... (default: standard)")
        (@arg mode: --mode +takes_value          "Alias for profile")
        (@arg whatsin: --whatsin +takes_value    "Defines the provided input chunk, choose from document (default), fragment and formula")
        (@arg whatsout: --whatsout +takes_value  "Defines the expected output chunk, choose from document (default), fragment and formula")
        (@arg post: --post                       "requests a followup post-processing")
        (@arg nopost: --nopost                   "forbids followup post-processing")
        (@arg validate: --validate               "Enables (the default) or disables validation of the source xml.")
        (@arg novalidate: --novalidate           "disables validation of the source xml.")
        (@arg omitdoctype: --omitdoctype         "omits the Doctype declaration,")
        (@arg noomitdoctype: --noomitdoctype     "disables the omission (the default)")
        (@arg numbersections: --numbersections    "enables (the default) the inclusion of section numbers in titles, crossrefs.")
        (@arg nonumbersections: --nonumbersections "disables the above")
        (@arg timestamp: --timestamp             "provides a timestamp (typically a time and date) to be embedded in the comments")
        (@arg stylesheet: --stylesheet           "specifies a stylesheet, to be used by the post-processor.")
        (@arg css: --css +takes_value            "adds a css stylesheet to html/xhtml (can be repeated)")
        (@arg nodefaultresources: --nodefaultresources "disables processing built-in resources")
        (@arg javscript: --javscript +takes_value "adds a link to a javascript file into html/html5/xhtml (can be repeated)")
        (@arg icon: --icon +takes_value          "specify a file to use as a \"favicon\"")
        (@arg xsltparameter: --xsltparameter +takes_value "passes parameters to the XSLT.")
        (@arg split: --split                     "requests splitting each document")
        (@arg nosplit: --nosplit                 "disables the above (default)")
        (@arg splitat: --splitat                 "sets level to split the document")
        (@arg splitpath: --splitpath +takes_value "sets xpath expression to use for splitting (default splits at sections, if splitting is enabled)")
        (@arg splitnaming: --splitnaming +takes_value "(id|idrelative|label|labelrelative) specifies how to name split files (idrelative).")
        (@arg scan: --scan                       "scans documents to extract ids, labels, etc. section titles, etc. (default)")
        (@arg noscan: --noscan                   "disables the above")
        (@arg crossref: --crossref               "fills in crossreferences (default)")
        (@arg nocrossref: --nocrossref           "disables the above")
        (@arg urlstyle: --urlstyle +takes_value  "(server|negotiated|file) format to use for urls (default server).")
        (@arg navigationtoc: --navigationtoc +takes_value "(context|none) generates a table of contents in navigation bar")
        (@arg index: --index                     "requests creating an index (default)")
        (@arg noindex: --noindex                 "disables the above")
        (@arg splitindex: --splitindex           "Splits index into pages per initial.")
        (@arg nosplitindex: --nosplitindex       "disables the above (default)")
        (@arg permutedindex: --permutedindex     "permutes index phrases in the index")
        (@arg nopermutedindex: --nopermutedindex "disables the above (default)")
        (@arg bibliography: --bibliography +takes_value  "sets a bibliography file")
        (@arg splitbibliography: --splitbibliography     "splits the bibliography into pages per initial.")
        (@arg nosplitbibliography: --nosplitbibliography "disables the above (default)")
        (@arg prescan: --prescan  "carries out only the split (if enabled) and scan, storing cross-referencing data in dbfile (default is complete processing")
        (@arg dbfile: --dbfile +takes_value      "sets file to store crossreferences")
        (@arg mathimages: --mathimages           "converts math to images (default for html4 format)")
        (@arg nomathimages: --nomathimages       "disables the above")
        (@arg mathimagemagnification: --mathimagemagnification +takes_value "specifies magnification factor")
        (@arg linelength: --linelength +takes_value         "formats presentation mathml to a linelength max of n characters")
        (@arg openmath: --openmath               "converts math to OpenMath")
        (@arg noopenmath: --noopenmath           "disables the above (default)")
        (@arg mathtex: --mathtex                 "adds TeX annotation to parallel markup")
        (@arg nomathtex: --nomathtex             "disables the above (default)")
        (@arg plane1: --plane1                   "use plane-1 unicode for symbols (default, if needed)")
        (@arg noplane1: --noplane1               "do not use plane-1 unicode")
        (@arg graphicimages: --graphicimages     "converts graphics to images (default)")
        (@arg nographicimages: --nographicimages "disables the above")
        (@arg graphicsmap: --graphicsmap +takes_value "specifies a graphics file mapping")
        (@arg pictureimages: --pictureimages     "converts picture environments to images (default)")
        (@arg nopictureimages: --nopictureimages "disables the above")
        (@arg svg: --svg                         "converts picture environments to SVG")
        (@arg nosvg: --nosvg                     "disables the above (default)")
        (@arg nocomments: --nocomments           "omit comments from the output")
        (@arg inputencoding: --inputencoding +takes_value  "specify the input encoding.")
        (@arg debug: --debug +takes_value        "enables debugging output for the named package")
     ).get_matches();

  let from_port: u16 = if let Some(port_str) = matches.value_of("PORT") {
    port_str.parse().unwrap()
  } else {
    3334
  };

  let input_file = matches.value_of("INPUT").unwrap().to_string();
  let output_file = matches.value_of("OUTPUT").unwrap().to_string();
  let log_file = matches.value_of("LOG").unwrap_or("runner.log").to_string();
  let autoflush = matches
    .value_of("autoflush")
    .unwrap_or("0")
    .parse::<usize>()
    .unwrap_or(0);
  matches.args.remove("PORT");
  matches.args.remove("INPUT");
  matches.args.remove("OUTPUT");
  matches.args.remove("LOG");
  matches.args.remove("autoflush");
  let mut boot_latexmls_opts = Vec::new();
  // clap option parsing mangles order, so we'll just impose the standard one for requested math
  // pmml is primary, followed by cmml, mathtex,
  let mut deferred_math = HashSet::new();
  for key in matches.args.keys() {
    let mut name_only = true;
    for val in matches.values_of(key).unwrap() {
      name_only = false;
      boot_latexmls_opts.push((key.to_string(), val.to_string()));
    }
    if name_only {
      match *key {
        "pmml" | "cmml" | "openmath" | "mathtex" | "nopmml" | "nocmml" | "noopenmath"
        | "nomathtex" => {
          deferred_math.insert(*key);
        }
        _ => boot_latexmls_opts.push((key.to_string(), String::new())),
      }
    }
  }
  for math_key in &[
    "pmml",
    "cmml",
    "openmath",
    "mathtex",
    "nopmml",
    "nocmml",
    "noopenmath",
    "nomathtex",
  ] {
    if deferred_math.contains(math_key) {
      boot_latexmls_opts.push((math_key.to_string(), String::new()))
    }
  }

  let mut harness = Harness::new(from_port, autoflush, boot_latexmls_opts)?;
  harness.convert_file(&input_file, &output_file, &log_file)
}
