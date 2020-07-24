use std::collections::HashSet;
use std::path::Path;
use std::fs::File;
use std::io::Write;

use super::structures::*;
use super::json_emit::*;
use super::js_emit::*;
use super::error::EmitError;
use super::typify_graph::ModuleGraph;
use super::config::EmitConfig;
use crate::compile_opt::CompileOpt;


struct Context<'a> {
    json_output: JsonOutput<'a>,
    js_output: JsOutput<'a>,
}

pub fn emit(
    options: &CompileOpt,
    root_module_path: &CanonPath,
    typed_graph: &ModuleGraph
) -> Result<(), EmitError> {

    let outdir = &options.output_dir;

    let file_name = options.file_stem
        .as_ref()
        .map(|f| std::ffi::OsStr::new(f))
        .unwrap_or_else(|| {
            root_module_path
                .as_path()
                .file_stem()
                .expect("Root module info path has no filename")
        });

    let mut context = Context {
        json_output: JsonOutput::new(&options),
        js_output: JsOutput::new(&options),
    };

    traverse(
        options,
        root_module_path,
        typed_graph,
        &mut context,
    );

    opt!(options.emit_config, json, {

        let json_output_path = {
            let mut output_path = outdir.to_owned();
            output_path.push(file_name);
            output_path.set_extension("arr.json");

            output_path
        };

        // Emit JSON into file
        let root_path = root_module_path.as_path().to_owned();
        let mut file =
            File::create(json_output_path)
            .map_err(|io_err| EmitError::IoError(root_path.to_owned(), io_err))?;

        let output = context.json_output
            .finalize()
            .map_err(|json_err| EmitError::JsonError(root_path.to_owned(), json_err))?;

        file.write_all(output.as_bytes())
            .map_err(|io_err| EmitError::IoError(root_path.to_owned(), io_err))?;

    });

    opt!(options.emit_config, js, {

        let js_output_path = {
            let mut output_path = outdir.to_owned();
            output_path.push(file_name);
            output_path.set_extension("arr.js");

            output_path
        };

        // Emit JS into file
        let root_path = root_module_path.as_path();
        let default_require_path: String = {
            use std::path::PathBuf;
            let mut buff = PathBuf::new();
            buff.push("./");
            buff.push(root_path.file_stem().unwrap());
            buff.set_extension("js");

            buff.display().to_string()
        };
        let mut file =
            File::create(js_output_path)
            .map_err(|io_err| EmitError::IoError(root_path.to_owned(), io_err))?;

        let output = context.js_output
            .finalize(default_require_path);

        file.write_all(output.as_bytes())
            .map_err(|io_err| EmitError::IoError(root_path.to_owned(), io_err))?;

    });

    Ok(())
}

fn traverse(
    options: &CompileOpt,
    root: &CanonPath,
    graph: &ModuleGraph,
    context: &mut Context,
) {
    let mut visited: HashSet<&CanonPath> = HashSet::new();

    let mut stack: Vec<&CanonPath> = vec![root];

    while stack.is_empty() == false {
        let node_path = stack.pop().unwrap();

        if visited.contains(node_path) {
            continue;
        }
        visited.insert(node_path);

        let node = graph.nodes.get(node_path).unwrap();

        opt!(options.emit_config, json, {
            for (export_key, typ) in node.rooted_export_types.iter() {
                context.json_output.export_type(export_key, typ);
            }

            for (export_key, typ) in node.rooted_export_values.iter() {
                context.json_output.export_value(export_key, typ);
            }
        });


        opt!(options.emit_config, js, {
            for (export_key, typ) in node.rooted_export_types.iter() {
                context.js_output.handle_type(export_key, typ);
            }

            for (export_key, typ) in node.rooted_export_values.iter() {
                context.js_output.handle_value(export_key, typ);
            }
        });

        let edges = graph.export_edges.get(node_path).unwrap();

        for edge in edges {
            stack.push(edge.export_source());
        }
    }
}