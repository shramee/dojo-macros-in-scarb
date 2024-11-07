use cairo_lang_macro::{attribute_macro, Diagnostic, ProcMacroResult, TokenStream};
use cairo_lang_parser::utils::SimpleParserDatabase;
use cairo_lang_syntax::node::ast::{self, MaybeModuleBody, ModuleItem};
use cairo_lang_syntax::node::db::SyntaxGroup;
use cairo_lang_syntax::node::kind::SyntaxKind;
use cairo_lang_syntax::node::{SyntaxNode, Terminal, TypedSyntaxNode};

const CONTRACT_PATCH: &str = include_str!("patches/contract.patch.cairo");
const DEFAULT_INIT_PATCH: &str = include_str!("patches/default_init.patch.cairo");
const CONSTRUCTOR_FN: &str = "constructor";
const DOJO_INIT_FN: &str = "dojo_init";

#[attribute_macro]
pub fn contract(_attr: TokenStream, item: TokenStream) -> ProcMacroResult {
    let db = SimpleParserDatabase::default();

    item.

    if let SyntaxKind::ItemModule = module_ast.kind(&db) {
        let children_ast = module_ast.descendants(db);
        children_ast
            .filter_map(|node| {
                if let SyntaxKind::ItemModule = node.kind(&db) {
                    Some(node)
                } else {
                    None
                }
            })
            .for_each(|node| {
                let name = node.name(&db).text(&db);
                let diagnostics = vec![Diagnostic::error(
                    format!(
                        "The contract module '{}' cannot contain nested modules.",
                        name
                    )
                )].into();

                return ProcMacroResult::new(item).with_diagnostics(diagnostics);
            });
        let name = module.name(&db).text(&db);

        let diagnostics = vec![Diagnostic::error(
            format!(
                "The contract name '{}' can only contain characters (a-z/A-Z), digits (0-9) and underscore (_).",
                name
            )
        )].into();

        // Check module name validity
        if !is_name_valid(&name) {
            return ProcMacroResult::new(item).with_diagnostics(diagnostics);
        }

        // Process module body
        let mut body_nodes = Vec::new();
        let mut has_event = false;
        let mut has_storage = false;
        let mut has_init = false;
        let mut has_constructor = false;

        if let MaybeModuleBody::Some(body) = module.body(&db) {
            for item in body.items(&db) {
                match item {
                    ModuleItem::Enum(ref enum_ast) => {
                        if enum_ast.name(&db).text(&db) == "Event" {
                            has_event = true;
                            // Add processed event node
                            body_nodes.push(process_event(&db, enum_ast));
                        }
                    }
                    ModuleItem::Struct(ref struct_ast) => {
                        if struct_ast.name(&db).text(&db) == "Storage" {
                            has_storage = true;
                            // Add processed storage node
                            body_nodes.push(process_storage(&db, struct_ast));
                        }
                    }
                    ModuleItem::FreeFunction(ref fn_ast) => {
                        let fn_name = fn_ast.declaration(&db).name(&db).text(&db);
                        if fn_name == CONSTRUCTOR_FN {
                            has_constructor = true;
                            // Add processed constructor
                            body_nodes.extend(process_constructor(&db, fn_ast));
                        } else if fn_name == DOJO_INIT_FN {
                            has_init = true;
                            // Add processed init function
                            body_nodes.extend(process_init(&db, fn_ast));
                        }
                    }
                    _ => body_nodes.push(item.as_syntax_node().get_text(&db)),
                }
            }
        }

        // Add default implementations if missing
        if !has_constructor {
            body_nodes.push(
                "
                #[constructor]
                fn constructor(ref self: ContractState) {
                    self.world_provider.initializer();
                }
                "
                .to_string(),
            );
        }

        if !has_init {
            body_nodes.push(DEFAULT_INIT_PATCH.replace("$init_name$", DOJO_INIT_FN));
        }

        if !has_event {
            body_nodes.push(
                "
                #[event]
                #[derive(Drop, starknet::Event)]
                enum Event {
                    UpgradeableEvent: upgradeable_cpt::Event,
                    WorldProviderEvent: world_provider_cpt::Event,
                }
                "
                .to_string(),
            );
        }

        if !has_storage {
            body_nodes.push(
                "
                #[storage]
                struct Storage {
                    #[substorage(v0)]
                    upgradeable: upgradeable_cpt::Storage,
                    #[substorage(v0)]
                    world_provider: world_provider_cpt::Storage,
                }
                "
                .to_string(),
            );
        }

        // Combine body nodes
        let body = body_nodes.join("\n");

        // Generate final code using the contract patch
        let final_code = CONTRACT_PATCH
            .replace("$name$", &name)
            .replace("$body$", &body);

        ProcMacroResult::new(TokenStream::new(&final_code))
    } else {
        ProcMacroResult::new(item).with_diagnostics(vec![Diagnostic::error(
            "Contract macro can only be applied to modules",
        )])
    }
}

fn process_event(db: &dyn SyntaxGroup, enum_ast: &ast::ItemEnum) -> String {
    let variants = enum_ast
        .variants(db)
        .elements(db)
        .iter()
        .map(|v| v.as_syntax_node().get_text(db))
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        "
        #[event]
        #[derive(Drop, starknet::Event)]
        enum Event {{
            UpgradeableEvent: upgradeable_cpt::Event,
            WorldProviderEvent: world_provider_cpt::Event,
            {}
        }}
        ",
        variants
    )
}

fn process_storage(db: &dyn SyntaxGroup, struct_ast: &ast::ItemStruct) -> String {
    let members = struct_ast
        .members(db)
        .elements(db)
        .iter()
        .map(|m| m.as_syntax_node().get_text(db))
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        "
        #[storage]
        struct Storage {{
            #[substorage(v0)]
            upgradeable: upgradeable_cpt::Storage,
            #[substorage(v0)]
            world_provider: world_provider_cpt::Storage,
            {}
        }}
        ",
        members
    )
}

fn process_constructor(db: &dyn SyntaxGroup, fn_ast: &ast::FunctionWithBody) -> Vec<String> {
    let declaration = fn_ast.declaration(db);
    let params = declaration
        .signature(db)
        .parameters(db)
        .elements(db)
        .iter()
        .map(|p| p.as_syntax_node().get_text(db))
        .collect::<Vec<_>>()
        .join(", ");

    let mut nodes = vec![format!(
        "
            #[constructor]
            fn constructor({}) {{
                self.world_provider.initializer();
            ",
        params
    )];

    // Add function body statements
    for stmt in fn_ast.body(db).statements(db).elements(db) {
        nodes.push(stmt.as_syntax_node().get_text(db));
    }

    nodes.push("}".to_string());
    nodes
}

fn process_init(db: &dyn SyntaxGroup, fn_ast: &ast::FunctionWithBody) -> Vec<String> {
    let declaration = fn_ast.declaration(db);
    let params = declaration
        .signature(db)
        .parameters(db)
        .elements(db)
        .iter()
        .map(|p| p.as_syntax_node().get_text(db))
        .collect::<Vec<_>>()
        .join(", ");

    let mut nodes = vec![
        "#[abi(per_item)]".to_string(),
        "#[generate_trait]".to_string(),
        "pub impl IDojoInitImpl of IDojoInit {".to_string(),
        "#[external(v0)]".to_string(),
        format!("fn {}({}) {{", DOJO_INIT_FN, params),
        "if starknet::get_caller_address() != self.world_provider.world_dispatcher().contract_address {
            core::panics::panic_with_byte_array(@format!(
                \"Only the world can init contract `{}`, but caller is `{:?}`\",
                self.dojo_name(),
                starknet::get_caller_address()
            ));
        }".to_string(),
    ];

    // Add function body statements
    for stmt in fn_ast.body(db).statements(db).elements(db) {
        nodes.push(stmt.as_syntax_node().get_text(db));
    }

    nodes.push("}".to_string());
    nodes.push("}".to_string());
    nodes
}

fn is_name_valid(name: &str) -> bool {
    name.chars().all(|c| c.is_alphanumeric() || c == '_')
}
