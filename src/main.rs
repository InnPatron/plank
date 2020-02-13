#[macro_use]
mod macros;
mod error;
mod structures;
mod type_structs;
mod init_type_scope;
mod type_construction;
mod bind_init;
mod bind_common;
mod bind_graph_init;
mod graph_reduce;
mod typify_graph;
// mod emit;
// mod emit_structures;

use std::sync::Arc;
use std::path::PathBuf;

use swc_common::{
    errors::{ColorConfig, Handler},
    FileName, FilePathMapping, SourceMap,
};
use swc_ecma_parser::{lexer::Lexer, Parser, Session, SourceFileInput, Syntax, TsConfig, JscTarget};

use clap::{Arg, App};

use error::EmitError;

fn output_directory_validator(arg: String) -> Result<(), String> {
    if PathBuf::from(arg).is_dir() {
        Ok(())
    } else {
        Err("Expected output argument to be a directory".to_string())
    }
}

fn main() {

    let matches = App::new("plank")
        .arg(Arg::with_name("INPUT")
            .short("i")
            .long("input")
            .value_name("ROOT_MODULE")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("OUTPUT")
            .short("o")
            .long("output")
            .value_name("DIR_PATH")
            .takes_value(true)
            .required(true)
            .validator(output_directory_validator))
        .get_matches();

    let input_path =
        matches.value_of("INPUT").expect("No input root module");

    let output_dir =
        matches.value_of("OUTPUT").expect("No output directory");

    let output_dir = PathBuf::from(output_dir);
    let input_path = PathBuf::from(input_path);

    swc_common::GLOBALS.set(&swc_common::Globals::new(), move || {
        let cm: Arc<SourceMap> = Default::default();
        let handler =
            Handler::with_tty_emitter(ColorConfig::Auto, true, false,
Some(cm.clone()));

        let session = Session {
            handler: &handler,
        };

        let cache = match bind_init::init(cm.clone(), session, input_path) {
            Ok(c) => c,

            Err(e) => {
                eprintln!("module cache error: {:?}", e);
                std::process::exit(1);
            }
        };

        let graph = match bind_graph_init::init(&cache) {
            Ok(g) => g,

            Err(e) => {
                eprintln!("graph init error: {:?}", e);
                std::process::exit(1);
            }
        };

        let graph = match graph_reduce::reduce(graph) {
            Ok(g) => g,

            Err(e) => {
                eprintln!("graph reduction error: {:?}", e);
                std::process::exit(1);
            }
        };

        let typed_graph = match typify_graph::typify(&cache, graph) {
            Ok(g) => g,

            Err(e) => {
                eprintln!("bind-gen error: {:?}", e);
                std::process::exit(1);
            }
        };

        //let context =
        //    bind_gen::Context::new(
        //        input_path,
        //        &handler,
        //        cm.clone(),
        //    );
        //let mut bind_gen_session = bind_gen::BindGenSession::new();

        //let module_info = (move || {
        //    bind_gen_session.bind_root_module(context)
        //})();

        //let module_info = match module_info {
        //    Ok(module_info) => module_info,

        //    Err(e) => {

        //        eprintln!("bind-gen error: {:?}", e);
        //        std::process::exit(1);
        //    }
        //};

        //let emit_result: Result<(), EmitError> = (move || {
        //    let _ = emit::emit_json(&output_dir, &module_info)?;

        //    Ok(())
        //})();

        //if let Err(e) = emit_result {
        //    eprintln!("emit error: {:?}", e);
        //    std::process::exit(1);
        //}
    });
}
