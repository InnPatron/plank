use std::collections::{HashMap, HashSet};

use swc_atoms::JsWord;
use swc_common::Span;
use swc_ecma_ast::*;

use super::bind_init::{ModuleData, ParsedModuleCache as ModuleCache};
use super::error::*;
use super::structures::{CanonPath, ItemState};

pub fn init(cache: &ModuleCache) -> Result<ModuleGraph, BindGenError> {
    let mut graph = ModuleGraph {
        nodes: HashMap::new(),
        export_edges: HashMap::new(),
        import_edges: HashMap::new(),
    };

    for (_, module_data) in cache.iter() {
        NodeInitSession::init(&mut graph, cache, module_data)?;
    }

    Ok(graph)
}

pub struct ModuleNode {
    pub path: CanonPath,
    pub rooted_export_types: HashSet<JsWord>,
    pub rooted_export_values: HashSet<JsWord>,
}

impl ModuleNode {
    pub fn is_rooted_type(&self, key: &JsWord) -> bool {
        self.rooted_export_types.contains(key)
    }

    pub fn is_rooted_value(&self, key: &JsWord) -> bool {
        self.rooted_export_values.contains(key)
    }
}

pub enum Import {
    NamedType { source: CanonPath, src_key: JsWord },
    NamedValue { source: CanonPath, src_key: JsWord },
    Named { source: CanonPath, src_key: JsWord },
}

#[derive(Debug)]
pub enum Export {
    NamedType {
        source: CanonPath,
        src_key: JsWord,
        export_key: JsWord,
    },
    NamedValue {
        source: CanonPath,
        src_key: JsWord,
        export_key: JsWord,
    },
    Named {
        source: CanonPath,
        src_key: JsWord,
        export_key: JsWord,
    },
    All {
        source: CanonPath,
    },
}

impl Export {
    pub fn export_source(&self) -> &CanonPath {
        match self {
            Export::NamedType { ref source, .. } => source,
            Export::NamedValue { ref source, .. } => source,
            Export::Named { ref source, .. } => source,
            Export::All { ref source, .. } => source,
        }
    }
}

/// ORDER OF EXPORTS MATTERS
/// ORDER OF IMPORTS MATTERS
///
/// Ordered by occurence in the AST
pub struct ModuleGraph {
    pub nodes: HashMap<CanonPath, ModuleNode>,
    pub export_edges: HashMap<CanonPath, Vec<Export>>,
    pub import_edges: HashMap<CanonPath, Vec<Import>>,
}

impl ModuleGraph {
    fn module_instantiated(&self, p: &CanonPath) -> bool {
        self.nodes.contains_key(p)
    }
}

#[derive(Copy, Clone)]
pub enum ScopeKind {
    Value,
    Type,
    ValueType,
}

struct NodeInitSession<'a> {
    path: &'a CanonPath,
    dependency_map: &'a HashMap<String, CanonPath>,
    import_edges: Vec<Import>,
    export_edges: Vec<Export>,
    rooted_values: HashSet<JsWord>,
    rooted_types: HashSet<JsWord>,

    value_scope: HashMap<JsWord, ItemState>,
    type_scope: HashMap<JsWord, ItemState>,
}

impl<'a> NodeInitSession<'a> {
    fn init(
        g: &mut ModuleGraph,
        cache: &ModuleCache,
        module_data: &ModuleData,
    ) -> Result<(), BindGenError> {
        let mut session = NodeInitSession {
            path: &module_data.path,
            dependency_map: &module_data.dependencies,
            import_edges: Vec::new(),
            export_edges: Vec::new(),
            rooted_values: HashSet::new(),
            rooted_types: HashSet::new(),

            value_scope: HashMap::new(),
            type_scope: HashMap::new(),
        };

        for item in module_data.module_ast.body.iter() {
            session.process_module_item(item)?;
        }

        let rooted_export_types = session.rooted_types;
        let rooted_export_values = session.rooted_values;
        let import_edges = session.import_edges;
        let export_edges = session.export_edges;

        let module_node = ModuleNode {
            path: module_data.path.clone(),
            rooted_export_types,
            rooted_export_values,
        };

        g.nodes.insert(module_data.path.clone(), module_node);

        g.export_edges
            .insert(module_data.path.clone(), export_edges);
        g.import_edges
            .insert(module_data.path.clone(), import_edges);

        Ok(())
    }

    fn scope_item(&mut self, name: JsWord, state: ItemState, kind: ScopeKind) {
        use std::collections::hash_map::Entry;

        match kind {
            ScopeKind::Value => match self.value_scope.entry(name) {
                Entry::Vacant(vacant) => {
                    vacant.insert(state);
                }
                Entry::Occupied(ref mut occupied) => (),
            },

            ScopeKind::Type => match self.type_scope.entry(name) {
                Entry::Vacant(vacant) => {
                    vacant.insert(state);
                }
                Entry::Occupied(ref mut occupied) => (),
            },

            ScopeKind::ValueType => {
                match self.type_scope.entry(name.clone()) {
                    Entry::Vacant(vacant) => {
                        vacant.insert(state.clone());
                    }
                    Entry::Occupied(ref mut occupied) => (),
                }

                match self.value_scope.entry(name) {
                    Entry::Vacant(vacant) => {
                        vacant.insert(state);
                    }
                    Entry::Occupied(ref mut occupied) => (),
                }
            }
        }
    }

    fn process_module_item(&mut self, item: &ModuleItem) -> Result<(), BindGenError> {
        match item {
            ModuleItem::ModuleDecl(ref decl) => self.process_module_decl(decl),

            ModuleItem::Stmt(ref stmt) => self.process_stmt(stmt),
        }
    }

    fn process_stmt(&mut self, stmt: &Stmt) -> Result<(), BindGenError> {
        if let Stmt::Decl(ref decl) = stmt {
            self.process_decl(decl, false)?;
        }

        Ok(())
    }

    fn process_module_decl(&mut self, module_decl: &ModuleDecl) -> Result<(), BindGenError> {
        match module_decl {
            ModuleDecl::Import(ref import) => {
                let src_canon_path: &CanonPath = get_dep_src!(self, import.src);

                for specifier in import.specifiers.iter() {
                    self.handle_import_specifier(src_canon_path, specifier)?;
                }

                Ok(())
            }

            ModuleDecl::ExportDecl(ExportDecl { ref decl, .. }) => self.process_decl(decl, true),

            ModuleDecl::ExportNamed(ref exp) => self.process_named_export(exp),

            ModuleDecl::ExportAll(ExportAll { ref src, .. }) => {
                let dep_canon_path = get_dep_src!(self, src);
                self.export_edges.push(Export::All {
                    source: dep_canon_path.clone(),
                });

                Ok(())
            }

            ModuleDecl::ExportDefaultDecl(..) => unreachable!("Caught in bind init"),

            ModuleDecl::ExportDefaultExpr(..) => unreachable!("Caught in bind init"),

            ModuleDecl::TsImportEquals(..) => unreachable!("Caught in bind init"),

            ModuleDecl::TsExportAssignment(..) => unreachable!("Caught in bind init"),

            ModuleDecl::TsNamespaceExport(..) => unreachable!("Caught in bind init"),
        }
    }

    fn prune_export_specifiers<'c, T>(
        &self,
        specifiers: T,
        exp_span: &Span,
    ) -> Result<Vec<&'c ExportSpecifier>, BindGenError>
    where
        T: Iterator<Item = &'c ExportSpecifier>,
    {
        let mut buff = Vec::new();
        for spec in specifiers {
            match spec {
                ExportSpecifier::Named(..) => {
                    buff.push(spec);
                }

                ExportSpecifier::Namespace(ExportNamespaceSpecifier { ref span, .. }) => {
                    return Err(BindGenError {
                        module_path: self.path.as_path().to_owned(),
                        kind: BindGenErrorKind::UnsupportedFeature(
                            UnsupportedFeature::NamespaceExport,
                        ),
                        span: span.clone(),
                    });
                }

                ExportSpecifier::Default(..) => {
                    return Err(BindGenError {
                        module_path: self.path.as_path().to_owned(),
                        kind: BindGenErrorKind::UnsupportedFeature(
                            UnsupportedFeature::DefaultExport,
                        ),
                        span: exp_span.clone(),
                    });
                }
            }
        }

        Ok(buff)
    }

    fn process_named_export(&mut self, exp: &NamedExport) -> Result<(), BindGenError> {
        let specifiers = self.prune_export_specifiers(exp.specifiers.iter(), &exp.span)?;

        match exp.src {
            Some(ref src) => {
                let src_canon_path: &CanonPath = get_dep_src!(self, src);

                // Add export edges
                for spec in specifiers {
                    match spec {
                        ExportSpecifier::Named(ExportNamedSpecifier {
                            ref orig,
                            exported: ref exported_as,
                            ..
                        }) => {
                            let orig_key = orig.sym.clone();
                            let export_key = exported_as
                                .as_ref()
                                .map(|x| x.sym.clone())
                                .unwrap_or(orig_key.clone());

                            self.export_edges.push(Export::Named {
                                source: src_canon_path.clone(),
                                src_key: orig_key,
                                export_key,
                            });
                        }

                        _ => unreachable!("Invalid specifier should be pruned"),
                    }
                }

                Ok(())
            }

            None => {
                for spec in specifiers {
                    match spec {
                        ExportSpecifier::Named(ExportNamedSpecifier {
                            ref orig,
                            exported: ref exported_as,
                            ..
                        }) => {
                            let orig_key = orig.sym.clone();
                            let export_key = exported_as
                                .as_ref()
                                .map(|x| x.sym.clone())
                                .unwrap_or(orig_key.clone());

                            // Handle the named export if it refers to a rooted item or imported
                            //   item by adding an edge if it is an imported item
                            //   or by marking the item as rooted (under its export key)

                            // Handle value
                            if let Some(ref state) = self.value_scope.get(&orig_key) {
                                match state {
                                    ItemState::Imported {
                                        ref source,
                                        ref src_key,
                                        ref as_key,
                                    } => {
                                        self.export_edges.push(Export::Named {
                                            source: source.clone(),
                                            src_key: src_key.clone(),
                                            export_key: as_key.clone(),
                                        });
                                    }

                                    ItemState::Rooted => {
                                        self.rooted_values.insert(export_key.clone());
                                    }
                                }
                            }

                            // Handle value
                            if let Some(ref state) = self.type_scope.get(&orig_key) {
                                match state {
                                    ItemState::Imported {
                                        ref source,
                                        ref src_key,
                                        ref as_key,
                                    } => {
                                        self.export_edges.push(Export::Named {
                                            source: source.clone(),
                                            src_key: src_key.clone(),
                                            export_key: as_key.clone(),
                                        });
                                    }

                                    ItemState::Rooted => {
                                        self.rooted_types.insert(export_key);
                                    }
                                }
                            }
                        }

                        _ => unreachable!("Invalid specifier should be pruned"),
                    }
                }

                Ok(())
            }
        }
    }

    fn process_decl(&mut self, decl: &Decl, export: bool) -> Result<(), BindGenError> {
        let (symbols, scope_kind) = match decl {
            Decl::Class(ClassDecl { ref ident, .. }) => {
                (vec![ident.sym.clone()], ScopeKind::ValueType)
            }

            Decl::Fn(FnDecl { ident, .. }) => (vec![ident.sym.clone()], ScopeKind::Value),

            Decl::Var(VarDecl { decls, .. }) => {
                let mut symbols = Vec::new();
                decls.iter().for_each(|decl| match decl.name {
                    Pat::Ident(ref ident) => {
                        symbols.push(ident.sym.clone());
                    }

                    _ => todo!("Handle all patterns"),
                });

                (symbols, ScopeKind::Value)
            }

            Decl::TsInterface(TsInterfaceDecl { id, .. }) => {
                (vec![id.sym.clone()], ScopeKind::Type)
            }

            Decl::TsTypeAlias(TsTypeAliasDecl { id, .. }) => {
                (vec![id.sym.clone()], ScopeKind::Type)
            }

            Decl::TsEnum(TsEnumDecl { id, .. }) => (vec![id.sym.clone()], ScopeKind::Type),

            Decl::TsModule(m) => {
                todo!(
                    "TS modules not suppported: {}::{:?}",
                    self.path.as_path().display(),
                    m.id
                );
            }
        };

        for symbol in symbols.into_iter() {
            match scope_kind {
                ScopeKind::Value => {
                    if export {
                        self.rooted_values.insert(symbol.clone());
                    }

                    self.scope_item(symbol, ItemState::Rooted, scope_kind);
                }

                ScopeKind::Type => {
                    if export {
                        self.rooted_types.insert(symbol.clone());
                    }
                    self.scope_item(symbol, ItemState::Rooted, scope_kind);
                }

                ScopeKind::ValueType => {
                    if export {
                        self.rooted_types.insert(symbol.clone());
                        self.rooted_values.insert(symbol.clone());
                    }
                    self.scope_item(symbol, ItemState::Rooted, scope_kind);
                }
            }
        }

        Ok(())
    }

    fn handle_import_specifier(
        &mut self,
        source: &CanonPath,
        spec: &ImportSpecifier,
    ) -> Result<(), BindGenError> {
        match spec {
            ImportSpecifier::Named(ref named) => {
                let src_key = named
                    .imported
                    .as_ref()
                    .map(|export_key| export_key.sym.clone())
                    .unwrap_or(named.local.sym.clone());

                let as_key = named.local.sym.clone();

                self.import_edges.push(Import::Named {
                    source: source.clone(),
                    src_key: src_key.clone(),
                });

                let item = ItemState::Imported {
                    source: source.clone(),
                    src_key,
                    as_key,
                };

                let import_key = named.local.sym.clone();
                self.scope_item(import_key, item, ScopeKind::ValueType);

                Ok(())
            }

            ImportSpecifier::Default(def) => Err(BindGenError {
                module_path: self.path.as_path().to_owned(),
                kind: BindGenErrorKind::UnsupportedFeature(UnsupportedFeature::DefaultImport),
                span: def.span,
            }),

            ImportSpecifier::Namespace(namespace) => Err(BindGenError {
                module_path: self.path.as_path().to_owned(),
                kind: BindGenErrorKind::UnsupportedFeature(UnsupportedFeature::DefaultImport),
                span: namespace.span,
            }),
        }
    }
}
