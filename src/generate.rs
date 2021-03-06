#[macro_use]
mod macros;
mod bind_common;
mod bind_graph_init;
mod bind_init;
mod config;
mod emit;
mod error;
mod graph_reduce;
mod init_type_scope;
mod js_pp;
mod structures;
mod ts_flavor_compat;
mod ts_flavor_detector;
mod type_construction;
mod type_structs;
mod typify_graph;

use std::sync::Arc;

use swc_common::{
    errors::{ColorConfig, Handler},
    SourceMap,
};

pub use self::config::EmitConfig;
pub use self::config::GenConfig;

use crate::compile_opt;
use crate::ts::TsFlavor;

pub fn gen(options: compile_opt::CompileOpt) {
    swc_common::GLOBALS.set(&swc_common::Globals::new(), move || {
        let cm: Arc<SourceMap> = Default::default();
        let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));

        let cache = match bind_init::init(cm.clone(), handler, options.input_path.clone()) {
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
                eprintln!("typify error: {:?}", e);
                std::process::exit(1);
            }
        };

        let detected_ts = ts_flavor_detector::detect(&typed_graph);

        let target_ts = options.ts_flavor.features();

        if let Err(e) = ts_flavor_compat::compatible(&detected_ts, &target_ts) {
            eprintln!("Compatibility errors:");
            for err in e {
                eprintln!("\t{:?}", err);
            }
            std::process::exit(1);
        }

        let result = match options.ts_flavor {
            TsFlavor::TsNum => emit::ts_num_emit(&options, &cache.root, &typed_graph),

            TsFlavor::TsFull => emit::ts_full_emit(&options, &cache.root, &typed_graph),

            TsFlavor::TsCustom(..) => todo!("TsCustom"),
        };

        match result {
            Ok(..) => (),

            Err(e) => {
                eprintln!("json-emit error: {:?}", e);
                std::process::exit(1);
            }
        }
    });
}
