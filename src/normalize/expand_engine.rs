// Expand engine — PYAX_SURFACE_EXPAND_V1 d2.
//
// A pure AST -> AST rewrite between Stage 5 (schema AST) and the Program split.
// It runs only when a surface passes `--expand <yaml>`; absent, the stage does
// not exist and every other surface is untouched.
//
// WHAT IT DOES
//   1. Type dispatch. `a + b`, `len(s)`, `s.upper()`, `xs[i]` become concrete
//      registry calls chosen from overload tables in the YAML. The types come
//      from what the author wrote — literals, parameter and `x: T`
//      annotations, same-file `def` return types — carried forward through
//      locals bottom-up (T2), plus the first-store rule: an empty `[]` takes
//      its element type from the first value stored into it. It never reads
//      the registry and never accepts or rejects a program on type grounds the
//      verifier could judge: when it cannot choose, it says which annotation
//      would let it, and everything it does choose is checked again by the
//      verifier, the only checker.
//   2. Control flow. A statement block becomes `bindings + one result`. An
//      expression statement is a binding to a reserved name; a statement `if`
//      that does not return on every path takes the rest of the block into its
//      branches; `return` ends the block.
//   3. Rebinding. `x = x + 1`, `x += 1` and natural updates (`xs.append(v)`,
//      `xs[i] = v`) bind a fresh name; later reads use it. An update that
//      Python would make visible through another name (a parameter, a second
//      name for the same list, the list a loop is iterating) is refused.
//   4. Loops. `for` lowers to the foreign `fold`, `while` to `loop_while`
//      followed by a check that faults if its bound ran out; a comprehension
//      is a `fold` in expression position. Each loop body is lifted to a named
//      top-level sibling fn taking `(state, item)`; the values the body reads
//      or updates travel in a packed state value, so nothing is captured. A
//      `return` inside a loop sets a done flag in the state; later iterations
//      skip, and the caller returns after the loop. No recursion is generated.
//
// SURFACE-AGNOSTIC. The engine knows roles, not PyAx: `kinds:` maps each role
// to the surface's node kind, and the fields each role carries are fixed by the
// contract below. A surface that projects these shapes can use the engine.
//
//   role            fields
//   program         items: Nodes
//   fn              name: Token, params: Node(param_list), return_type: Node(ret_ann), body: Node(block)
//   param_list      params: Nodes(param | param_rest | empty)
//   param           name: Token, type_expr: Node(type)
//   param_rest      param: Node(param)
//   ret_ann         type: Nodes(0 or 1 type)
//   named_type      name: Token
//   generic_type    name: Token, args: Nodes(type | type_rest)
//   type_rest       type_expr: Node(type)
//   block           bindings: Nodes(stmt), result: Node(stmt)
//   assign          name: Token, value: Node(expr)
//   ann_assign      name: Token, type_expr: Node(type), value: Node(expr)
//   aug_assign      name: Token, op: Token (`+=`), value: Node(expr)
//   sub_assign      name: Token, index: Node(expr), value: Node(expr)
//   return          expr: Node(expr)
//   pass / break / continue   -
//   expr_stmt       expr: Node(expr)
//   if_stmt / elif  condition: Node(expr), then_branch: Node(block), orelse: Node(elif | else | empty)
//   else            block: Node(block)
//   for_stmt        target: Node(for_target), iter: Node(expr), body: Node(block)
//   for_target      names: Tokens(1 or 2)
//   while_stmt      condition: Node(expr), body: Node(block)
//   cond_expr       value: Node(expr), tail: Nodes(0 or 1 cond_tail)
//   cond_tail       condition: Node(expr), orelse: Node(expr)
//   chain           first: Node(expr), rest: Nodes(chain_item)       (every kind in `chains`)
//   chain_item      op: Token, operand: Node(expr)                    (`not in` = op token `not`)
//   compare         first, rest — as chain; Python chaining semantics
//   not / neg       operand: Node(expr)
//   postfix         atom: Node(expr), trailers: Nodes(trailer)
//   call_trailer    args: Node(arg_list | gen_expr)
//   method_trailer  name: Token, args: Node(arg_list)
//   index_trailer   index: Node(expr)
//   arg_list        args: Nodes(expr | arg_rest | empty)
//   arg_rest        arg: Node(expr)
//   name            name: Token
//   int/text/bool/none literal   value: Token (none: -)
//   paren           inner: Node(expr)
//   list_lit        args: Node(arg_list)
//   list_comp / gen_expr  elem: Node(expr), target: Node(for_target), iter: Node(expr), cond: Nodes(0 or 1 comp_if)
//   comp_if         condition: Node(expr)
//
// Output kinds are the input kinds wherever one exists, so the output is the
// same surface's AST (EXPAND_OUTPUT_IS_PYAX): every dispatched operator becomes
// the `call_out` kind — an ordinary call, which the surface can write itself —
// and a lifted loop body is an ordinary `fn`.

use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use crate::frontend::token::{Span, Token, TokenKind};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// ── spec ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandSpec {
    pub version: String,
    /// Source in this same surface, compiled into every program ahead of the
    /// author's items. Relative to the YAML file.
    #[serde(default)]
    pub prelude: Option<String>,
    /// Names the author may not use; the engine mints its own under it.
    pub reserved_prefix: String,
    /// role -> node kind. See the contract above.
    pub kinds: BTreeMap<String, String>,
    /// Kinds with a single `variant` child that carry no meaning of their own.
    #[serde(default)]
    pub transparent: Vec<String>,
    /// Binary chain kinds (left-associative, `first` + `rest`).
    #[serde(default)]
    pub chains: Vec<String>,
    pub types: TypeRules,
    /// Lexeme rewrites applied to literal tokens (`True` -> `true`).
    #[serde(default)]
    pub canonical_lexemes: BTreeMap<String, String>,
    /// op lexeme (`+`, `==`, `and`, `not`, `neg`, ...) -> overloads.
    #[serde(default)]
    pub operators: BTreeMap<String, Vec<Overload>>,
    /// builtin call name -> overloads, chosen by argument types.
    #[serde(default)]
    pub builtins: BTreeMap<String, Vec<Overload>>,
    /// `Type.method` -> overloads; the receiver is `$0`. `list[*].m` matches any list.
    #[serde(default)]
    pub methods: BTreeMap<String, Vec<Overload>>,
    pub lists: ListRules,
    pub dicts: DictRules,
    pub slices: SliceRules,
    pub text: TextRules,
    pub loops: LoopRules,
    /// Builtins that accept several arguments by converting each with `str`
    /// and joining them with the given separator (Python's `print(a, b)`).
    #[serde(default)]
    pub joined_builtins: BTreeMap<String, String>,
    /// Builtins that, given a dict, work on its keys (Python iterates keys).
    #[serde(default)]
    pub dict_args_as_keys: Vec<String>,
    /// Modules whose import statements are accepted and ignored (`typing`).
    #[serde(default)]
    pub ignored_imports: Vec<String>,
    /// `raise E(msg)` / a failed `assert`: `$0` is the message text. Must fault.
    pub raise: String,
    /// Python truthiness per type (`$0` -> Bool); `list[*]`/`dict[*]` for any.
    /// A type absent here has no truth value. Bool is itself.
    #[serde(default)]
    pub truthy: BTreeMap<String, String>,
    /// A float literal: `$0` is its lexeme as a text literal (`str_to_float($0)`).
    pub float_literal: String,
    pub float_type: String,
    /// Modules whose functions (`math.sqrt` in `builtins`) and constants
    /// (`module_constants`) the surface provides.
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub module_constants: BTreeMap<String, Overload>,
    /// Sorting tuples: an order-preserving text key per element type, and the
    /// sort that orders a list by such keys (`$0` list, `$1` keys, `$2` reverse).
    #[serde(default)]
    pub sort_keys: BTreeMap<String, String>,
    pub sort_by_keys: String,
    /// Python's repr() of a scalar inside a container (`$0` -> Text): a text
    /// is quoted there. Containers are built from these by the engine.
    #[serde(default)]
    pub repr: BTreeMap<String, String>,
    /// The builtin that turns a value into text (`str`); with a container
    /// argument it — and every joined builtin (`print`) — uses repr.
    pub str_builtin: String,
    /// call name -> the message that says what to write instead.
    #[serde(default)]
    pub forbidden_calls: BTreeMap<String, String>,
    /// statement role (`break`, `continue`) -> the message.
    #[serde(default)]
    pub forbidden_statements: BTreeMap<String, String>,
    /// `<ns>.f(args)` calls registry `f` directly, bypassing builtins.
    #[serde(default)]
    pub escape_namespace: Option<String>,
    /// Function names the platform reserves, renamed at definition, call and
    /// reference: the bridge exports a fn under its own name, so a fn named
    /// `main` collides with the C entry symbol at link time.
    #[serde(default)]
    pub renames: BTreeMap<String, String>,
    /// The program's entry fn (after renames). The platform always passes
    /// it the command line, so an entry written with no parameters
    /// (`def main() -> None`) gets an unused one of `entry_param_type`.
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub entry_param_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeRules {
    /// Surface spelling -> canonical type name, e.g. `int: Int`.
    #[serde(default)]
    pub names: BTreeMap<String, String>,
    /// The generic constructors as spelled (`list`, `tuple`, `dict`); a list
    /// erases to `list_axis`, a tuple and a dict to `value`.
    pub list: String,
    pub list_axis: String,
    pub tuple: String,
    pub dict: String,
    /// Other spellings of the generic constructors (`List: list`).
    #[serde(default)]
    pub generic_aliases: BTreeMap<String, String>,
    /// The type a function without a return annotation returns.
    pub no_return: String,
    /// The erased type that loop state and list elements travel as.
    pub value: String,
    /// Literal kinds' types.
    pub int: String,
    pub text: String,
    pub bool: String,
    pub none: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overload {
    pub args: Vec<String>,
    /// A type, `$0` (the type of argument 0) or `elem$0` (its element type).
    pub ret: String,
    /// Call template: `int_add($0, $1)`, `bool_not(int_eq($0, $1))`.
    pub emit: String,
}

/// Templates for list values (erased to `types.list_axis`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRules {
    /// `$*` = the elements; also the empty list.
    pub new: String,
    /// Element `$1` of list `$0`, as the erased element (unpacked after).
    pub get: String,
    /// Normalise a possibly negative index `$1` against length `$0`.
    pub index: String,
    pub len: String,
    pub append: String,
    pub extend: String,
    pub set: String,
    /// `xs.pop()`: the last element (erased), and the list without it.
    pub pop_value: String,
    pub pop_rest: String,
    /// `del xs[i]`.
    pub del: String,
    /// `xs.insert(i, v)`.
    pub insert: String,
}

/// Templates for dicts (erased to `types.value`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DictRules {
    /// From a keys list `$0` and a values list `$1`.
    pub new: String,
    /// `$0[$1]` (erased value; faults on a missing key).
    pub get: String,
    /// `$0.get($1, $2)` (erased value).
    pub get_or: String,
    pub set: String,
    pub has: String,
    pub del: String,
    pub len: String,
    pub keys: String,
    pub values: String,
    /// A list of `(key, value)` tuples.
    pub items: String,
    /// `$0 | $1`, also `{**$0, ...}`.
    pub merge: String,
}

/// Python slices: bounds are normalised and clamped by the templates.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SliceRules {
    /// `$0[$1:$2]` for a list / a text.
    pub list: String,
    pub text: String,
    /// The value an omitted end bound stands for (clamped to the length).
    pub open_end: i64,
    /// `$0[::-1]`.
    pub reverse_list: String,
    pub reverse_text: String,
    /// Any other step: `$0` sequence, `$1` lo, `$2` hi, `$3` step, `$4`/`$5` bounds written.
    pub step_list: String,
    pub step_text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRules {
    /// Character `$1` of text `$0`.
    pub get: String,
    pub len: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopRules {
    /// Pack `$*` into one state value; read field `$1` of state `$0`.
    pub pack: String,
    pub field: String,
    /// `fold($0 list, $1 init, $2 step)`.
    pub fold: String,
    /// `loop_while($0 init, $1 cond, $2 step, $3 bound)`.
    #[serde(rename = "while")]
    pub while_: String,
    pub while_bound: i64,
    /// Evaluated when the bound ran out with the condition still true. Must fault.
    pub exhausted: String,
    /// `range($0, $1)` over ints, as a list.
    pub range: String,
    /// Erased value -> typed, per type; `list[*]` for any list.
    pub unpack: BTreeMap<String, String>,
    /// A placeholder of each type, for the return slot of a loop's done flag.
    pub defaults: BTreeMap<String, String>,
}

const ROLES: &[&str] = &[
    "program", "fn", "param_list", "param", "param_rest", "ret_ann", "named_type",
    "generic_type", "type_rest", "block", "assign", "ann_assign", "aug_assign", "sub_assign",
    "return", "pass", "break", "continue", "expr_stmt", "if_stmt", "elif", "else", "for_stmt",
    "for_target", "while_stmt", "cond_expr", "cond_tail", "compare", "not", "neg", "inv", "postfix",
    "call_trailer", "method_trailer", "index_trailer", "arg_list", "arg_rest", "empty", "name",
    "int_lit", "text_lit", "bool_lit", "none_lit", "paren", "list_lit", "list_comp", "gen_expr",
    "comp_if", "call_out", "expr_list", "tuple_rest", "tuple_assign", "target_list", "dict_lit",
    "dict_item", "dict_rest", "slice_trailer", "slice_step", "aug_sub_assign", "del_stmt", "fstring",
    "fexpr", "import_stmt", "raise_stmt", "assert_stmt", "float_lit", "attr_trailer", "kw_arg", "module_doc", "dict_comp", "dict_spread",
];

#[derive(Debug, Clone)]
pub struct ExpandError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expand error at {}..{}: {}", self.span.start, self.span.end, self.message)
    }
}

impl std::error::Error for ExpandError {}

fn err<T>(message: impl Into<String>, span: &Span) -> Result<T, ExpandError> {
    Err(ExpandError { message: message.into(), span: span.clone() })
}

type Typed = (SchemaAstNode, Option<String>);

/// A registry function's signature, by type name. Expand types a call to a
/// registry function from it, as every surface's lowering resolves the same
/// call through the same scope.
#[derive(Clone, Debug)]
pub struct RegSig {
    pub params: Vec<String>,
    pub ret: String,
    /// `variadic true` in the registry: arity and types are the verifier's.
    pub variadic: bool,
}

/// Registry function name -> signature (built from the merged scope).
pub type RegistrySigs = HashMap<String, RegSig>;

pub struct ExpandEngine {
    spec: ExpandSpec,
    dir: PathBuf,
    role_of: HashMap<String, String>,
    /// Round-trip mode only: names under the reserved prefix are accepted.
    allow_reserved: bool,
}

impl ExpandEngine {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read expand spec {}: {}", path.display(), e))?;
        let spec: ExpandSpec = serde_yaml::from_str(&text)
            .map_err(|e| format!("expand spec {}: {}", path.display(), e))?;
        if spec.version != "0.1" {
            return Err(format!("expand spec version '{}' is not supported (0.1)", spec.version));
        }
        for role in ROLES {
            if !spec.kinds.contains_key(*role) {
                return Err(format!("expand spec: `kinds` has no entry for role `{}`", role));
            }
        }
        for role in spec.kinds.keys() {
            if !ROLES.contains(&role.as_str()) {
                return Err(format!("expand spec: `kinds.{}` is not a role", role));
            }
        }
        let mut templates: Vec<(String, &String)> = Vec::new();
        for table in [&spec.operators, &spec.builtins, &spec.methods] {
            for (key, overloads) in table {
                for o in overloads {
                    templates.push((key.clone(), &o.emit));
                }
            }
        }
        let l = &spec.lists;
        for (k, t) in [("lists.new", &l.new), ("lists.get", &l.get), ("lists.index", &l.index), ("lists.len", &l.len),
            ("lists.append", &l.append), ("lists.extend", &l.extend), ("lists.set", &l.set),
            ("lists.pop_value", &l.pop_value), ("lists.pop_rest", &l.pop_rest), ("lists.del", &l.del), ("lists.insert", &l.insert), ("text.get", &spec.text.get),
            ("dicts.new", &spec.dicts.new), ("dicts.get", &spec.dicts.get), ("dicts.get_or", &spec.dicts.get_or),
            ("dicts.set", &spec.dicts.set), ("dicts.has", &spec.dicts.has), ("dicts.del", &spec.dicts.del),
            ("dicts.len", &spec.dicts.len), ("dicts.keys", &spec.dicts.keys), ("dicts.values", &spec.dicts.values),
            ("dicts.items", &spec.dicts.items), ("dicts.merge", &spec.dicts.merge), ("slices.list", &spec.slices.list), ("slices.text", &spec.slices.text),
            ("slices.reverse_list", &spec.slices.reverse_list), ("slices.reverse_text", &spec.slices.reverse_text),
            ("slices.step_list", &spec.slices.step_list), ("slices.step_text", &spec.slices.step_text),
            ("raise", &spec.raise), ("float_literal", &spec.float_literal), ("sort_by_keys", &spec.sort_by_keys),
            ("text.len", &spec.text.len), ("loops.pack", &spec.loops.pack), ("loops.field", &spec.loops.field),
            ("loops.fold", &spec.loops.fold), ("loops.while", &spec.loops.while_),
            ("loops.exhausted", &spec.loops.exhausted), ("loops.range", &spec.loops.range)] {
            templates.push((k.to_string(), t));
        }
        for (k, o) in &spec.module_constants {
            templates.push((k.clone(), &o.emit));
        }
        for (k, t) in spec.loops.unpack.iter().chain(spec.loops.defaults.iter()).chain(spec.truthy.iter()).chain(spec.sort_keys.iter()).chain(spec.repr.iter()) {
            templates.push((k.clone(), t));
        }
        for (key, t) in templates {
            parse_template(t).map_err(|e| format!("expand spec `{}`: {}", key, e))?;
        }
        let mut role_of = HashMap::new();
        for (role, kind) in &spec.kinds {
            role_of.insert(kind.clone(), role.clone());
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(ExpandEngine { spec, dir, role_of, allow_reserved: false })
    }

    /// The engine for re-compiling expand's own output, printed as source:
    /// no prelude (the printed program already holds it) and the reserved
    /// names expand minted are accepted. EXPAND_OUTPUT_IS_PYAX is the claim
    /// that this compiles to the same bundles as the original source.
    pub fn for_expanded_source(&self) -> ExpandEngine {
        let mut spec = self.spec.clone();
        spec.prelude = None;
        ExpandEngine { spec, dir: self.dir.clone(), role_of: self.role_of.clone(), allow_reserved: true }
    }

    pub fn prelude_path(&self) -> Option<PathBuf> {
        self.spec.prelude.as_ref().map(|p| self.dir.join(p))
    }

    /// Printing expand's output as source: a call it emitted names a
    /// registry or user fn, so a name the surface reads as something else (a
    /// builtin, a refused call) must be written through the escape namespace
    /// (`axis.range(...)`) to keep its meaning.
    pub fn printed_call_name(&self, name: &str) -> String {
        let shadowed = self.spec.builtins.contains_key(name)
            || self.spec.forbidden_calls.contains_key(name)
            || self.spec.joined_builtins.contains_key(name);
        match (&self.spec.escape_namespace, shadowed) {
            (Some(ns), true) => format!("{}.{}", ns, name),
            _ => name.to_string(),
        }
    }

    /// The name the author wrote for a (possibly renamed) fn.
    pub fn author_name<'a>(&'a self, name: &'a str) -> &'a str {
        self.spec.renames.iter().find(|(_, to)| to.as_str() == name).map(|(from, _)| from.as_str()).unwrap_or(name)
    }

    /// A fn as a diagnostic should name it: synthetic names never appear
    /// alone — a lifted body is "a loop in `f`" (SPANS_SURVIVE_EXPAND).
    pub fn describe_fn(&self, name: &str) -> String {
        if let Some(rest) = name.strip_prefix(&self.spec.reserved_prefix) {
            for (tag, what) in [("_loop", "a loop body"), ("_cond", "a while condition"), ("_comp", "a comprehension")] {
                if let Some(i) = rest.rfind(tag) {
                    return format!("{} in `{}`", what, self.author_name(&rest[..i]));
                }
            }
        }
        format!("`{}`", self.author_name(name))
    }

    fn kind(&self, role: &str) -> &str {
        &self.spec.kinds[role]
    }

    fn role<'a>(&'a self, node: &SchemaAstNode) -> &'a str {
        self.role_of.get(&node.kind).map(String::as_str).unwrap_or("")
    }

    fn renamed(&self, t: &Token) -> Token {
        match self.spec.renames.get(&t.lexeme) {
            Some(to) => Token::new(t.kind.clone(), to.clone(), t.span.clone()),
            None => t.clone(),
        }
    }

    fn reserved(&self, t: &Token) -> Result<(), ExpandError> {
        if !self.allow_reserved && t.lexeme.starts_with(&self.spec.reserved_prefix) {
            return err(format!("names starting with `{}` are reserved", self.spec.reserved_prefix), &t.span);
        }
        Ok(())
    }

    // ── types ──────────────────────────────────────────────────────────────

    /// `head[a, b, ...]` -> (head, [a, b, ...]), splitting at top-level commas.
    fn generic(t: &str) -> Option<(&str, Vec<&str>)> {
        let open = t.find('[')?;
        let inner = t.strip_suffix(']')?.get(open + 1..)?;
        let mut args = Vec::new();
        let (mut depth, mut start) = (0i32, 0usize);
        for (i, c) in inner.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                ',' if depth == 0 => {
                    args.push(&inner[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        args.push(&inner[start..]);
        Some((&t[..open], args))
    }

    fn head(t: &str) -> &str {
        t.split('[').next().unwrap_or(t)
    }

    fn is_list(t: &str) -> bool {
        Self::head(t) == "list" && t.contains('[')
    }

    fn is_tuple(t: &str) -> bool {
        Self::head(t) == "tuple" && t.contains('[')
    }

    fn is_dict(t: &str) -> bool {
        Self::head(t) == "dict" && t.contains('[')
    }

    fn elem_of(t: &str) -> Option<&str> {
        t.strip_prefix("list[").and_then(|r| r.strip_suffix(']'))
    }

    /// A dict's (key, value) types, when known.
    fn dict_kv(t: &str) -> Option<(String, String)> {
        match Self::generic(t) {
            Some(("dict", a)) if a.len() == 2 => Some((a[0].to_string(), a[1].to_string())),
            _ => None,
        }
    }

    fn tuple_elems(t: &str) -> Option<Vec<String>> {
        match Self::generic(t) {
            Some(("tuple", a)) => Some(a.into_iter().map(String::from).collect()),
            _ => None,
        }
    }

    /// Pending element types (`list[?3]`, `dict[?4]`) resolved through the
    /// first-store table, recursively.
    fn resolve(&self, t: &str, cx: &FnCx) -> String {
        let (head, args) = match Self::generic(t) {
            Some(g) => g,
            None => return t.to_string(),
        };
        if args.len() == 1 {
            if let Some(k) = args[0].strip_prefix('?') {
                if let Some(Some(e)) = cx.pending.get(k) {
                    return self.resolve(&format!("{}[{}]", head, e), cx);
                }
                return t.to_string();
            }
        }
        let args: Vec<String> = args.iter().map(|a| self.resolve(a, cx)).collect();
        format!("{}[{}]", head, args.join(","))
    }

    fn resolved(&self, t: &Option<String>, cx: &FnCx) -> Option<String> {
        t.as_ref().map(|t| self.resolve(t, cx))
    }

    fn axis_type(&self, t: &str) -> String {
        if Self::is_list(t) {
            self.spec.types.list_axis.clone()
        } else if Self::is_tuple(t) || Self::is_dict(t) {
            self.spec.types.value.clone()
        } else {
            t.to_string()
        }
    }

    /// A canonical type as the author spells it (`list[Int]` -> `list[int]`), for messages.
    fn spell(&self, t: &str) -> String {
        if let Some((head, args)) = Self::generic(t) {
            let h = match head {
                "list" => self.spec.types.list.clone(),
                "tuple" => self.spec.types.tuple.clone(),
                "dict" => self.spec.types.dict.clone(),
                other => other.to_string(),
            };
            let args: Vec<String> = args.iter().map(|a| if a.starts_with('?') { "?".to_string() } else { self.spell(a) }).collect();
            return format!("{}[{}]", h, args.join(", "));
        }
        self.spec.types.names.iter().find(|(_, c)| c.as_str() == t).map(|(s, _)| s.clone()).unwrap_or_else(|| t.to_string())
    }

    fn spell_all(&self, tys: &[String]) -> String {
        tys.iter().map(|t| self.spell(t)).collect::<Vec<_>>().join(", ")
    }

    fn type_node(&self, t: &str, span: &Span) -> SchemaAstNode {
        let tok = Token::new(TokenKind::Ident, self.axis_type(t), span.clone());
        mk(self.kind("named_type"), span, vec![("name", SchemaValue::Token(tok))])
    }

    /// A type annotation -> its canonical type (`list[Int]`, `dict[Text,Int]`, `Int`, ...).
    fn type_of_annotation(&self, t: &SchemaAstNode) -> Result<String, ExpandError> {
        let t = self.unwrap(t);
        if t.kind == self.kind("named_type") {
            let n = &token(t, "name")?.lexeme;
            return Ok(self.spec.types.names.get(n).cloned().unwrap_or_else(|| n.clone()));
        }
        if t.kind == self.kind("generic_type") {
            let spelled = &token(t, "name")?.lexeme;
            let name = self.spec.types.generic_aliases.get(spelled).unwrap_or(spelled);
            let mut args = Vec::new();
            for a in nodes(t, "args") {
                let a = if a.kind == self.kind("type_rest") { node(a, "type_expr")? } else { a };
                args.push(self.type_of_annotation(a)?);
            }
            let ty = &self.spec.types;
            let canon = if name == &ty.list && args.len() == 1 {
                "list"
            } else if name == &ty.dict && args.len() == 2 {
                "dict"
            } else if name == &ty.tuple && !args.is_empty() {
                "tuple"
            } else {
                return err(format!("type `{}[...]` is not in this language (only {}[T], {}[K, V], {}[T, ...])", spelled, ty.list, ty.dict, ty.tuple), &t.span);
            };
            return Ok(format!("{}[{}]", canon, args.join(",")));
        }
        err(format!("not a type: {}", t.kind), &t.span)
    }

    fn params(&self, list: &SchemaAstNode) -> Result<Vec<(Token, String)>, ExpandError> {
        let mut out = Vec::new();
        for p in nodes(list, "params") {
            let p = if p.kind == self.kind("param_rest") { node(p, "param")? } else { p };
            if p.kind == self.kind("empty") {
                continue;
            }
            let name = token(p, "name")?.clone();
            self.reserved(&name)?;
            out.push((name, self.type_of_annotation(node(p, "type_expr")?)?));
        }
        Ok(out)
    }

    fn param_defaults(&self, list: &SchemaAstNode) -> Result<Vec<Option<SchemaAstNode>>, ExpandError> {
        let mut out = Vec::new();
        for p in nodes(list, "params") {
            let p = if p.kind == self.kind("param_rest") { node(p, "param")? } else { p };
            if p.kind == self.kind("empty") {
                continue;
            }
            out.push(match nodes(p, "default").first() {
                Some(d) => Some(node(d, "value")?.clone()),
                None => None,
            });
        }
        Ok(out)
    }

    fn ret_type(&self, ret: &SchemaAstNode) -> Result<String, ExpandError> {
        match nodes(ret, "type").first() {
            Some(t) => self.type_of_annotation(t),
            None => Ok(self.spec.types.no_return.clone()),
        }
    }

    fn matches(&self, pattern: &str, t: &str) -> bool {
        pattern == t
            || pattern == "*"
            || (pattern == "list[*]" && Self::is_list(t))
            || (pattern == "tuple[*]" && Self::is_tuple(t))
            || (pattern == "dict[*]" && Self::is_dict(t))
    }

    // ── program and functions ──────────────────────────────────────────────

    /// Expand a whole program: `prelude_items` (already projected, from the
    /// prelude source) go first, then the author's items, then every lifted fn.
    pub fn expand_program(
        &self,
        program: SchemaAstNode,
        prelude_items: Vec<SchemaAstNode>,
    ) -> Result<SchemaAstNode, ExpandError> {
        self.expand_program_with_registry(program, prelude_items, None)
    }

    /// `expand_program`, typing calls to registry functions from `registry`:
    /// their results get the declared type, and lists convert between PyAx's
    /// `list[T]` and the registry's `IntList` / `TextList` / `BoolList`.
    /// Off for expanded source (`for_expanded_source`), whose conversions are
    /// already written out: adding them again would change the bundles.
    pub fn expand_program_with_registry(
        &self,
        program: SchemaAstNode,
        prelude_items: Vec<SchemaAstNode>,
        registry: Option<&RegistrySigs>,
    ) -> Result<SchemaAstNode, ExpandError> {
        let registry = if self.allow_reserved { None } else { registry };
        let prelude_count = prelude_items.len();
        let mut items = prelude_items;
        if program.kind == self.kind("program") {
            items.extend(nodes(&program, "items").into_iter().cloned());
        } else {
            items.push(program.clone());
        }
        // `from typing import ...`: accepted and dropped; any other import is refused.
        let mut kept = Vec::with_capacity(items.len());
        for item in items {
            let it = self.unwrap(&item);
            if it.kind == self.kind("module_doc") {
                continue;
            }
            if it.kind == self.kind("import_stmt") {
                let m = token(it, "module")?;
                if !self.spec.ignored_imports.contains(&m.lexeme) {
                    return err(format!("imports are not in this language (only `from {} import ...`, which is ignored)", self.spec.ignored_imports.join("/")), &m.span);
                }
                continue;
            }
            kept.push(item);
        }
        // Module-level constants (`LIMIT = 100`): inlined where they are read.
        let mut consts: HashMap<String, (SchemaAstNode, Option<String>)> = HashMap::new();
        let items = kept;
        let mut kept = Vec::with_capacity(items.len());
        for item in items {
            let it = self.unwrap(&item);
            let r = self.role(it);
            if r == "assign" || r == "ann_assign" {
                let name = token(it, "name")?;
                self.reserved(name)?;
                let declared = if r == "ann_assign" { Some(self.type_of_annotation(node(it, "type_expr")?)?) } else { None };
                if consts.insert(name.lexeme.clone(), (node(it, "value")?.clone(), declared)).is_some() {
                    return err(format!("the constant `{}` is defined twice", name.lexeme), &name.span);
                }
                continue;
            }
            kept.push(item);
        }
        let items = kept;
        let mut sigs: HashMap<String, Sig> = HashMap::new();
        for item in &items {
            let item = self.unwrap(item);
            if item.kind == self.kind("fn") {
                let name = token(item, "name")?;
                self.reserved(name)?;
                let params = self.params(node(item, "params")?)?;
                let ret = self.ret_type(node(item, "return_type")?)?;
                let defaults = self.param_defaults(node(item, "params")?)?;
                sigs.insert(self.renamed(name).lexeme, Sig {
                    ret,
                    names: params.iter().map(|p| p.0.lexeme.clone()).collect(),
                    params: params.into_iter().map(|p| p.1).collect(),
                    defaults,
                });
            }
        }
        let mut out = Vec::with_capacity(items.len());
        for (idx, item) in items.iter().enumerate() {
            let item = self.unwrap(item);
            if item.kind != self.kind("fn") {
                return err(format!("only function definitions may appear at top level (got {})", item.kind), &item.span);
            }
            // A prelude span is an offset into the prelude file, not the
            // author's: say so, or the caller would map it onto the wrong line.
            let (f, lifted) = self.expand_fn(item, &sigs, &consts, registry).map_err(|e| if idx < prelude_count {
                ExpandError { message: format!("in prelude: {}", e.message), span: e.span }
            } else {
                e
            })?;
            out.push(f);
            out.extend(lifted);
        }
        Ok(mk(&program.kind, &program.span, vec![("items", SchemaValue::Nodes(out))]))
    }

    fn unwrap<'a>(&self, mut n: &'a SchemaAstNode) -> &'a SchemaAstNode {
        while self.spec.transparent.contains(&n.kind) {
            match n.fields.get("variant") {
                Some(SchemaValue::Node(inner)) => n = inner,
                _ => break,
            }
        }
        n
    }

    fn param_list(&self, params: &[(String, String)], span: &Span) -> SchemaAstNode {
        let ps = params.iter().map(|(n, t)| mk(self.kind("param"), span, vec![
            ("name", SchemaValue::Token(ident(n, span))),
            ("type_expr", SchemaValue::Node(Box::new(self.type_node(t, span)))),
        ])).collect();
        mk(self.kind("param_list"), span, vec![("params", SchemaValue::Nodes(ps))])
    }

    fn fn_node(&self, name: &str, params: &[(String, String)], ret: &str, body: SchemaAstNode, span: &Span) -> SchemaAstNode {
        mk(self.kind("fn"), span, vec![
            ("name", SchemaValue::Token(ident(name, span))),
            ("params", SchemaValue::Node(Box::new(self.param_list(params, span)))),
            ("return_type", SchemaValue::Node(Box::new(self.type_node(ret, span)))),
            ("body", SchemaValue::Node(Box::new(body))),
        ])
    }

    fn expand_fn(&self, f: &SchemaAstNode, sigs: &HashMap<String, Sig>, consts: &HashMap<String, (SchemaAstNode, Option<String>)>, registry: Option<&RegistrySigs>) -> Result<(SchemaAstNode, Vec<SchemaAstNode>), ExpandError> {
        let name = self.renamed(token(f, "name")?);
        let params = self.params(node(f, "params")?)?;
        let ret = self.ret_type(node(f, "return_type")?)?;
        let mut env = Env::default();
        for (pname, ty) in &params {
            env.vars.insert(pname.lexeme.clone(), Var { ty: Some(ty.clone()), cur: pname.lexeme.clone(), param: true, shared: None, sentinel: None });
        }
        let stmts = self.block_stmts(node(f, "body")?);
        // Pass 1, discovery: learn every first-store element type, including
        // stores that come after a read (`seen = {}` ... `seen[c]` ... `seen[x] = j`).
        // Its output and its errors are discarded; only what it learned is kept.
        let mut probe = FnCx {
            sigs,
            ret: ret.clone(),
            fn_name: self.author_name(&name.lexeme).to_string(),
            counter: 0,
            fn_span: f.span.clone(),
            lifted: Vec::new(),
            pending: BTreeMap::new(),
            discover: true,
            int_sentinels: BTreeSet::new(),
            consts,
            const_stack: Vec::new(),
            hoisted: BTreeSet::new(),
            scoped: BTreeMap::new(),
            reg: registry,
        };
        let _ = self.block(&stmts, env.clone(), &mut probe, &Mode::Fn, Vec::new());
        // Sentinels can depend on each other (`best = max(best, x - low)` only
        // becomes int once `low` is): repeat discovery until nothing new turns up.
        for _ in 0..4 {
            let known = probe.int_sentinels.clone();
            let mut again = FnCx {
                sigs,
                ret: ret.clone(),
                fn_name: self.author_name(&name.lexeme).to_string(),
                counter: 0,
                fn_span: f.span.clone(),
                lifted: Vec::new(),
                pending: probe.pending.clone(),
                discover: true,
                int_sentinels: known.clone(),
                consts,
                const_stack: Vec::new(),
                hoisted: BTreeSet::new(),
            scoped: BTreeMap::new(),
            reg: registry,
            };
            let _ = self.block(&stmts, env.clone(), &mut again, &Mode::Fn, Vec::new());
            let grew = again.int_sentinels.len() > known.len();
            probe.int_sentinels = again.int_sentinels;
            probe.pending = again.pending;
            if !grew {
                break;
            }
        }
        // Pass 2: the expansion, with those types known from the start.
        let mut cx = FnCx {
            sigs,
            ret: ret.clone(),
            fn_name: self.author_name(&name.lexeme).to_string(),
            counter: 0,
            fn_span: f.span.clone(),
            lifted: Vec::new(),
            pending: probe.pending,
            discover: false,
            int_sentinels: probe.int_sentinels,
            consts,
            const_stack: Vec::new(),
            hoisted: BTreeSet::new(),
            scoped: BTreeMap::new(),
            reg: registry,
        };
        let body = self.block(&stmts, env, &mut cx, &Mode::Fn, Vec::new())?;
        let mut ps: Vec<(String, String)> = params.iter().map(|(n, t)| (n.lexeme.clone(), t.clone())).collect();
        if ps.is_empty() && self.spec.entry.as_deref() == Some(name.lexeme.as_str()) {
            if let Some(t) = &self.spec.entry_param_type {
                ps.push((format!("{}argv", self.spec.reserved_prefix), t.clone()));
            }
        }
        let out = self.fn_node(&name.lexeme, &ps, &ret, body, &f.span);
        Ok((out, cx.lifted))
    }

    // ── statements ─────────────────────────────────────────────────────────

    /// The statements of a block, in order, wrappers removed.
    fn block_stmts(&self, b: &SchemaAstNode) -> Vec<SchemaAstNode> {
        let b = self.unwrap(b);
        let mut out: Vec<SchemaAstNode> = nodes(b, "bindings").into_iter().map(|s| self.unwrap(s).clone()).collect();
        if let Ok(r) = node(b, "result") {
            out.push(self.unwrap(r).clone());
        }
        out
    }

    /// Does this statement list end every path in `return`?
    fn returns(&self, stmts: &[SchemaAstNode]) -> bool {
        let last = match stmts.iter().rev().find(|s| self.role(s) != "pass") {
            Some(s) => s,
            None => return false,
        };
        match self.role(last) {
            "return" | "raise_stmt" => true,
            "if_stmt" | "elif" => {
                let then_ok = node(last, "then_branch").map(|b| self.returns(&self.block_stmts(b))).unwrap_or(false);
                let else_ok = match nodes(last, "orelse").first() {
                    None => false,
                    Some(e) => self.returns(&self.else_stmts(e)),
                };
                then_ok && else_ok
            }
            _ => false,
        }
    }

    /// An `else` / `elif` clause as a statement list (`elif` is a one-statement `else`).
    fn else_stmts(&self, e: &SchemaAstNode) -> Vec<SchemaAstNode> {
        let e = self.unwrap(e);
        match self.role(e) {
            "else" => node(e, "block").map(|b| self.block_stmts(b)).unwrap_or_default(),
            "empty" => vec![],
            _ => vec![e.clone()],
        }
    }

    /// Statements -> `block{bindings, result}`.
    ///
    /// `prefix` are output bindings that come first (a lifted loop body's
    /// state unpacking). In `Mode::Loop`, falling off the end yields the packed
    /// state, and `return e` sets the done flag and yields it.
    fn block(&self, stmts: &[SchemaAstNode], mut env: Env, cx: &mut FnCx, mode: &Mode, prefix: Vec<SchemaAstNode>) -> Result<SchemaAstNode, ExpandError> {
        let mut b: Vec<SchemaAstNode> = prefix;
        let span = stmts.first().map(|s| s.span.clone()).unwrap_or_else(|| cx.fn_span.clone());
        for (i, st) in stmts.iter().enumerate() {
            // `print(xs.pop())`, `[.. for a, b in zip(xs, xs[1:])]`: bind the
            // part to a fresh name in a statement of its own, then expand
            // the rewritten statement and the rest as usual.
            if let Some((pre, rewritten)) = self.hoist_stmt(st, cx) {
                let mut v = pre;
                v.push(rewritten);
                v.extend(stmts[i + 1..].iter().cloned());
                return self.block(&v, env, cx, mode, b);
            }
            let role = self.role(st).to_string();
            match role.as_str() {
                "pass" => {}
                "fn" => return err("a nested `def` is not in PyAx: move it to the top level and pass what it uses as arguments", &st.span),
                "break" | "continue" => {
                    let msg = self.spec.forbidden_statements.get(&role).cloned().unwrap_or_else(|| format!("`{}` is not in this language", role));
                    return err(msg, &st.span);
                }
                "assign" | "ann_assign" => {
                    let name = token(st, "name")?.clone();
                    if !cx.hoisted.contains(&name.lexeme) {
                        self.reserved(&name)?;
                    }
                    let declared = if role == "ann_assign" { Some(self.type_of_annotation(node(st, "type_expr")?)?) } else { None };
                    let value_src = node(st, "value")?;
                    // `y = xs.pop()`: a value and an update.
                    if let Some((recv, m)) = self.update_call(value_src) {
                        if m == "pop" {
                            let (v, vty) = self.pop(&recv, value_src, &mut env, cx, &mut b)?;
                            self.assign(&name, declared, (v, vty), None, &mut env, cx, &mut b)?;
                            continue;
                        }
                    }
                    let shared = self.sharing(value_src, &env, cx);
                    let sentinel = if declared.is_none() { self.inf_sign(value_src) } else { None };
                    let value = match sentinel {
                        Some(sign) if cx.int_sentinels.contains(&name.lexeme) => {
                            // pyaxc's values: inf is i64::MAX and -inf its negation, so
                            // `x - low` stays in range for every int x of the same sign.
                            let v = if sign > 0 { i64::MAX } else { -i64::MAX };
                            (int_lit(self.kind("int_lit"), v, &value_src.span), Some(self.spec.types.int.clone()))
                        }
                        _ => self.expr_expect(value_src, declared.as_deref(), &mut env, cx)?,
                    };
                    self.assign(&name, declared, value, shared, &mut env, cx, &mut b)?;
                    if let Some(v) = env.vars.get_mut(&name.lexeme) {
                        v.sentinel = sentinel;
                    }
                }
                "aug_assign" => {
                    let name = token(st, "name")?.clone();
                    let op_tok = token(st, "op")?;
                    let op = op_tok.lexeme.trim_end_matches('=').to_string();
                    let cur = self.read_var(&name, &env, cx)?;
                    let value_src = node(st, "value")?;
                    // `total += stack.pop()`: the pop happens first, as Python evaluates it.
                    let rhs = match self.update_call(value_src) {
                        Some((recv, m)) if m == "pop" && recv.lexeme != name.lexeme => self.pop(&recv, value_src, &mut env, cx, &mut b)?,
                        _ => self.expr(value_src, &mut env, cx)?,
                    };
                    let is_list = cur.1.as_deref().map_or(false, |t| Self::is_list(t) || Self::is_dict(t));
                    let value = self.dispatch_op(&op, vec![cur, rhs], &op_tok.span, cx)?;
                    if is_list {
                        // `xs += ys` extends in place: an update, visible through other names.
                        self.update(&name, value, &mut env, cx, &mut b)?;
                    } else {
                        self.assign(&name, None, value, None, &mut env, cx, &mut b)?;
                    }
                }
                "sub_assign" | "aug_sub_assign" => {
                    let name = token(st, "name")?.clone();
                    let (_, xty) = self.read_var(&name, &env, cx)?;
                    let xty = xty.unwrap_or_default();
                    // The index is read twice for `d[k] += v`: bind it once.
                    let (mut key, kty) = self.expr(node(st, "index")?, &mut env, cx)?;
                    if role == "aug_sub_assign" && !self.simple(&key) {
                        let k = cx.fresh(&self.spec.reserved_prefix, "k");
                        b.push(self.binding(&k, key, &st.span));
                        key = self.name_node(&k, &st.span);
                    }
                    let mut value = self.expr(node(st, "value")?, &mut env, cx)?;
                    if role == "aug_sub_assign" {
                        let op_tok = token(st, "op")?;
                        let op = op_tok.lexeme.trim_end_matches('=').to_string();
                        let cur = self.subscript_read(&name, &xty, (key.clone(), kty.clone()), &env, cx, &st.span)?;
                        value = self.dispatch_op(&op, vec![cur, value], &op_tok.span, cx)?;
                    }
                    let new = self.subscript_write(&name, &xty, (key, kty), value, &env, cx, &st.span)?;
                    self.update(&name, new, &mut env, cx, &mut b)?;
                }
                "del_stmt" => {
                    let name = token(st, "name")?.clone();
                    let (xs, xty) = self.read_var(&name, &env, cx)?;
                    let xty = xty.unwrap_or_default();
                    let (key, kty) = self.expr(node(st, "index")?, &mut env, cx)?;
                    let new = if Self::is_dict(&xty) {
                        self.instantiate(&self.spec.dicts.del.clone(), &[xs, key], &st.span)?
                    } else if Self::is_list(&xty) {
                        if kty.as_deref() != Some(self.spec.types.int.as_str()) {
                            return err("a list index must be an int", &key.span);
                        }
                        self.instantiate(&self.spec.lists.del.clone(), &[xs, key], &st.span)?
                    } else {
                        return err(format!("`del {}[...]` needs a list or a dict; `{}` is {}", name.lexeme, name.lexeme, self.spell(&xty)), &name.span);
                    };
                    self.update(&name, (new, Some(xty)), &mut env, cx, &mut b)?;
                }
                "raise_stmt" => {
                    if let Some(next) = stmts[i + 1..].iter().find(|s| self.role(s) != "pass") {
                        return err("this code can never run: it comes after a `raise`", &next.span);
                    }
                    let msg = self.exception_message(node(st, "exc")?, &mut env, cx)?;
                    let fault = self.instantiate(&self.spec.raise.clone(), &[msg], &st.span)?;
                    return Ok(self.finish(b, fault, &span));
                }
                "assert_stmt" => {
                    let cond = self.cond(node(st, "condition")?, &mut env, cx)?;
                    let text = Some(self.spec.types.text.clone());
                    let label = self.text_lit("AssertionError", &st.span);
                    let msg = match nodes(st, "msg").first() {
                        Some(m) => {
                            let v = self.expr(node(m, "msg")?, &mut env, cx)?;
                            let v = self.dispatch_builtin("str", vec![v], &st.span, cx)?;
                            let sep = (self.text_lit("AssertionError: ", &st.span), text.clone());
                            self.dispatch_op("+", vec![sep, v], &st.span, cx)?.0
                        }
                        None => label,
                    };
                    let fault = self.instantiate(&self.spec.raise.clone(), &[msg], &st.span)?;
                    let ok = self.block(&stmts[i + 1..], env.clone(), cx, mode, Vec::new())?;
                    let result = self.if_node(cond, ok, self.finish(vec![], fault, &st.span), &st.span);
                    return Ok(self.finish(b, result, &span));
                }
                "tuple_assign" => {
                    let names: Vec<Token> = tokens(node(st, "target")?, "names");
                    for n in &names {
                        self.reserved(n)?;
                    }
                    let (v, vty) = self.expr(node(st, "value")?, &mut env, cx)?;
                    let elems = match vty.as_deref().and_then(Self::tuple_elems) {
                        Some(e) if e.len() == names.len() => e,
                        Some(e) => return err(format!("{} names on the left, but the value has {} parts", names.len(), e.len()), &st.span),
                        None => return err(format!("`a, b = ...` needs a tuple on the right; this is {}", vty.as_deref().map(|t| self.spell(t)).unwrap_or("of unknown type".into())), &st.span),
                    };
                    // Evaluate the right side once, before any name is rebound (`a, b = b, a`).
                    let t = cx.fresh(&self.spec.reserved_prefix, "t");
                    b.push(self.binding(&t, v, &st.span));
                    for (k, (n, ty)) in names.iter().zip(elems.iter()).enumerate() {
                        let field = self.instantiate(&self.spec.loops.field.clone(), &[self.name_node(&t, &st.span), int_lit(self.kind("int_lit"), k as i64, &st.span)], &st.span)?;
                        let part = self.unpack(ty, field, &st.span, cx)?;
                        self.assign(n, None, (part, Some(ty.clone())), None, &mut env, cx, &mut b)?;
                    }
                }
                "expr_stmt" => {
                    let e = node(st, "expr")?;
                    // A docstring (a bare string statement) has no effect.
                    if self.role(self.strip_wrappers(e)) == "text_lit" {
                        continue;
                    }
                    if let Some((recv, m)) = self.update_call(e) {
                        self.update_stmt(&recv, &m, e, &mut env, cx, &mut b)?;
                        continue;
                    }
                    let (value, _) = self.expr(e, &mut env, cx)?;
                    let fresh = cx.fresh(&self.spec.reserved_prefix, "d");
                    b.push(self.binding(&fresh, value, &st.span));
                }
                "return" => {
                    if let Some(next) = stmts[i + 1..].iter().find(|s| self.role(s) != "pass") {
                        return err("this code can never run: it comes after a `return`", &next.span);
                    }
                    let ret = cx.ret.clone();
                    let value = match st.fields.get("expr") {
                        Some(SchemaValue::Node(e)) => match self.update_call(e) {
                            // `return stack.pop()`
                            Some((recv, m)) if m == "pop" => self.pop(&recv, e, &mut env, cx, &mut b)?,
                            _ => self.expr_expect(e, Some(&ret), &mut env, cx)?,
                        },
                        // A bare `return`: None.
                        _ => {
                            if ret != self.spec.types.none {
                                return err(format!("a bare `return` returns None, but this function is declared to return {}", self.spell(&ret)), &st.span);
                            }
                            (mk(self.kind("none_lit"), &st.span, vec![]), Some(self.spec.types.none.clone()))
                        }
                    };
                    let result = self.do_return(value, &mut env, cx, mode, &mut b, &st.span)?;
                    return Ok(self.finish(b, result, &span));
                }
                "if_stmt" | "elif" => {
                    let rest = &stmts[i + 1..];
                    let cond = self.cond(node(st, "condition")?, &mut env, cx)?;
                    let mut then_stmts = self.block_stmts(node(st, "then_branch")?);
                    let mut else_stmts = match nodes(st, "orelse").first() {
                        Some(e) => self.else_stmts(e),
                        None => vec![],
                    };
                    {
                        let mut sets = Vec::new();
                        for side in [&then_stmts, &else_stmts] {
                            let (mut a, mut r, mut hr) = (BTreeSet::new(), BTreeSet::new(), false);
                            self.scan(side, &mut a, &mut r, &mut hr);
                            // A branch that returns doesn't reach the rest.
                            sets.push((a, self.returns(side)));
                        }
                        let (then_a, then_ret) = &sets[0];
                        let (else_a, else_ret) = &sets[1];
                        for n in then_a.iter().chain(else_a.iter()) {
                            let everywhere = (*then_ret || then_a.contains(n)) && (*else_ret || else_a.contains(n));
                            if !everywhere && !env.vars.contains_key(n) {
                                cx.scoped.entry(n.clone()).or_insert(Scoped::Branch);
                            }
                        }
                    }
                    if !self.returns(&then_stmts) {
                        then_stmts.extend(rest.iter().cloned());
                    }
                    if !self.returns(&else_stmts) {
                        else_stmts.extend(rest.iter().cloned());
                    }
                    let then_b = self.block(&then_stmts, env.clone(), cx, mode, Vec::new())?;
                    let else_b = self.block(&else_stmts, env.clone(), cx, mode, Vec::new())?;
                    let result = self.if_node(cond, then_b, else_b, &st.span);
                    return Ok(self.finish(b, result, &span));
                }
                "for_stmt" | "while_stmt" => {
                    let rest = &stmts[i + 1..];
                    let result = self.lower_loop(st, rest, &mut env, cx, mode, &mut b)?;
                    if let Some(result) = result {
                        return Ok(self.finish(b, result, &span));
                    }
                }
                "" => return err(format!("statement kind `{}` is not handled by expand", st.kind), &st.span),
                other => return err(format!("`{}` is not a statement", other), &st.span),
            }
        }
        // Fell off the end.
        let result = match mode {
            Mode::Loop(frame) => self.pack(frame, &env, &span, cx)?,
            Mode::Fn if cx.ret == self.spec.types.none => mk(self.kind("none_lit"), &span, vec![]),
            Mode::Fn => {
                return err(format!(
                    "this function can reach its end without returning a value (it is declared to return {}); add a `return` or an `else`",
                    self.spell(&cx.ret)
                ), &span_end(stmts, &cx.fn_span))
            }
        };
        Ok(self.finish(b, result, &span))
    }

    fn finish(&self, mut bindings: Vec<SchemaAstNode>, result: SchemaAstNode, span: &Span) -> SchemaAstNode {
        // A block is a statement list, not an expression: a nested block in
        // result position contributes its bindings, in order, and its result.
        let result = if result.kind == self.kind("block") {
            bindings.extend(nodes(&result, "bindings").into_iter().cloned());
            node(&result, "result").cloned().unwrap_or(result)
        } else {
            result
        };
        mk(self.kind("block"), span, vec![
            ("bindings", SchemaValue::Nodes(bindings)),
            ("result", SchemaValue::Node(Box::new(result))),
        ])
    }

    fn binding(&self, name: &str, value: SchemaAstNode, span: &Span) -> SchemaAstNode {
        mk(self.kind("assign"), span, vec![
            ("name", SchemaValue::Token(ident(name, span))),
            ("value", SchemaValue::Node(Box::new(value))),
        ])
    }

    fn if_node(&self, cond: SchemaAstNode, then_b: SchemaAstNode, else_b: SchemaAstNode, span: &Span) -> SchemaAstNode {
        mk(self.kind("if_stmt"), span, vec![
            ("condition", SchemaValue::Node(Box::new(cond))),
            ("then_branch", SchemaValue::Node(Box::new(then_b))),
            ("orelse", SchemaValue::Node(Box::new(else_b))),
        ])
    }

    fn name_node(&self, name: &str, span: &Span) -> SchemaAstNode {
        mk(self.kind("name"), span, vec![("name", SchemaValue::Token(ident(name, span)))])
    }

    /// An expression used as a condition: Python truthiness. `and` / `or` /
    /// `not` are boolean here, and every other operand is converted with the
    /// `truthy` table (`if xs:` is `if len(xs) > 0:`).
    fn cond(&self, e: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<SchemaAstNode, ExpandError> {
        let e = self.strip_wrappers(e);
        let bool_t = Some(self.spec.types.bool.clone());
        if self.spec.chains.contains(&e.kind) {
            let rest = nodes(e, "rest");
            let all_bool_ops = rest.iter().all(|r| token(r, "op").map_or(false, |t| t.lexeme == "and" || t.lexeme == "or"));
            if all_bool_ops && !rest.is_empty() {
                let mut acc = self.cond(node(e, "first")?, env, cx)?;
                for item in rest {
                    let op = token(item, "op")?;
                    let rhs = self.cond(node(item, "operand")?, env, cx)?;
                    acc = self.dispatch_op(&op.lexeme, vec![(acc, bool_t.clone()), (rhs, bool_t.clone())], &op.span, cx)?.0;
                }
                return Ok(acc);
            }
        }
        if self.role(e) == "not" {
            let x = self.cond(node(e, "operand")?, env, cx)?;
            return Ok(self.dispatch_op("not", vec![(x, bool_t)], &e.span, cx)?.0);
        }
        let v = self.expr(e, env, cx)?;
        self.truthy(v, cx)
    }

    /// A value as a Bool, by Python's truthiness rules.
    fn truthy(&self, v: Typed, cx: &mut FnCx) -> Result<SchemaAstNode, ExpandError> {
        let ty = match &v.1 {
            Some(t) => self.resolve(t, cx),
            // A registry call or a value whose type expand cannot see: used
            // as it is; the verifier requires a Bool condition.
            None => return Ok(v.0),
        };
        if ty == self.spec.types.bool {
            return Ok(v.0);
        }
        let key = if Self::is_list(&ty) { "list[*]".to_string() } else if Self::is_dict(&ty) { "dict[*]".to_string() } else { ty.clone() };
        match self.spec.truthy.get(&key) {
            Some(t) => {
                let span = v.0.span.clone();
                self.instantiate(&t.clone(), &[v.0], &span)
            }
            None => err(format!("{} has no truth value here; compare it explicitly", self.spell(&ty)), &v.0.span),
        }
    }

    /// `a and b` / `a or b` as a value. Two bools: boolean. Two values of one
    /// other type: Python's meaning — the operand itself (`name or "anon"`).
    /// Mixed types have no single type, so they are refused rather than
    /// turned into a bool the program did not ask for.
    fn bool_op_value(&self, op: &str, a: Typed, b: Typed, span: &Span, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let bool_t = self.spec.types.bool.clone();
        let ta = a.1.as_ref().map(|t| self.resolve(t, cx));
        let tb = b.1.as_ref().map(|t| self.resolve(t, cx));
        match (&ta, &tb) {
            (Some(x), Some(y)) if x == &bool_t && y == &bool_t => self.dispatch_op(op, vec![a, b], span, cx),
            (Some(x), Some(y)) if self.same_type(x, y, cx) => {
                if !self.simple(&a.0) {
                    return err(format!("`{}` would read its left side twice here; assign it to a name first", op), &a.0.span);
                }
                let test = self.truthy((a.0.clone(), ta.clone()), cx)?;
                let (then_, else_) = if op == "or" { (a.0, b.0) } else { (b.0, a.0) };
                Ok((mk(self.kind("cond_expr"), span, vec![
                    ("condition", SchemaValue::Node(Box::new(test))),
                    ("then_branch", SchemaValue::Node(Box::new(then_))),
                    ("orelse", SchemaValue::Node(Box::new(else_))),
                ]), ta))
            }
            (None, _) | (_, None) => self.soft(cx, format!("`{}` needs to know the types of its operands; annotate the variable they come from", op), span),
            (Some(x), Some(y)) => err(format!(
                "`{}` between {} and {} has no single type as a value; use it in a condition, or compare explicitly",
                op, self.spell(x), self.spell(y)
            ), span),
        }
    }

    /// `return value` — from the function, or, inside a lifted loop body,
    /// into the done flag and return slot of the loop state.
    fn do_return(&self, value: Typed, env: &mut Env, cx: &mut FnCx, mode: &Mode, b: &mut Vec<SchemaAstNode>, span: &Span) -> Result<SchemaAstNode, ExpandError> {
        match mode {
            Mode::Fn => Ok(value.0),
            Mode::Loop(frame) => {
                let p = &self.spec.reserved_prefix;
                let ret_ty = cx.ret.clone();
                self.assign(&ident(&format!("{}ret", p), span), Some(ret_ty), value, None, env, cx, b)?;
                let t = self.bool_lit(true, span);
                self.assign(&ident(&format!("{}done", p), span), Some(self.spec.types.bool.clone()), (t, Some(self.spec.types.bool.clone())), None, env, cx, b)?;
                self.pack(frame, env, span, cx)
            }
        }
    }

    /// A type that is not known (yet): an error, except in the discovery pass,
    /// where it is a placeholder so the pass can reach later stores.
    fn soft(&self, cx: &FnCx, message: String, span: &Span) -> Result<Typed, ExpandError> {
        if cx.discover {
            return Ok((mk(self.kind("none_lit"), span, vec![]), None));
        }
        err(message, span)
    }

    /// `float("inf")`, `float("-inf")`, `-float("inf")`, `math.inf`, `-math.inf`.
    fn inf_sign(&self, e: &SchemaAstNode) -> Option<i8> {
        let e = self.strip_wrappers(e);
        if self.role(e) == "neg" {
            return self.inf_sign(node(e, "operand").ok()?).map(|s| -s);
        }
        if self.role(e) == "postfix" {
            let atom = self.strip_wrappers(node(e, "atom").ok()?);
            let trailers = nodes(e, "trailers");
            if self.role(atom) == "name" && trailers.len() == 1 {
                let t = self.unwrap(trailers[0]);
                let n = &token(atom, "name").ok()?.lexeme;
                if self.role(t) == "attr_trailer" && self.spec.modules.contains(n) && token(t, "name").ok()?.lexeme == "inf" {
                    return Some(1);
                }
            }
        }
        let (f, args) = self.simple_call(e)?;
        if f != "float" || args.len() != 1 {
            return None;
        }
        let a = self.strip_wrappers(args[0]);
        if self.role(a) != "text_lit" {
            return None;
        }
        let lex = token(a, "value").ok()?.lexeme.trim_matches('"').trim().to_ascii_lowercase();
        match lex.as_str() {
            "inf" | "+inf" | "infinity" => Some(1),
            "-inf" | "-infinity" => Some(-1),
            _ => None,
        }
    }

    fn text_lit(&self, s: &str, span: &Span) -> SchemaAstNode {
        let tok = Token::new(TokenKind::StringLit, format!("\"{}\"", s), span.clone());
        mk(self.kind("text_lit"), span, vec![("value", SchemaValue::Token(tok))])
    }

    /// `raise E(msg)` -> the text `E: msg` (or `E`), as Python reports it.
    fn exception_message(&self, exc: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<SchemaAstNode, ExpandError> {
        let e = self.strip_wrappers(exc);
        let (name, args) = match self.simple_call(e) {
            Some((n, a)) => (n, a),
            None if self.role(e) == "name" => (token(e, "name")?.lexeme.clone(), vec![]),
            None => return err("`raise` needs an exception, e.g. `raise ValueError(\"why\")`", &exc.span),
        };
        let text = Some(self.spec.types.text.clone());
        match args.len() {
            0 => Ok(self.text_lit(&name, &e.span)),
            1 => {
                let v = self.expr(args[0], env, cx)?;
                let v = self.dispatch_builtin("str", vec![v], &e.span, cx)?;
                let head = (self.text_lit(&format!("{}: ", name), &e.span), text);
                Ok(self.dispatch_op("+", vec![head, v], &e.span, cx)?.0)
            }
            _ => err("`raise E(...)` takes one message", &e.span),
        }
    }

    fn bool_lit(&self, v: bool, span: &Span) -> SchemaAstNode {
        let lex = if v { "true" } else { "false" };
        mk(self.kind("bool_lit"), span, vec![("value", SchemaValue::Token(Token::new(TokenKind::BoolLit, lex.to_string(), span.clone())))])
    }

    // ── variables, rebinding, updates ──────────────────────────────────────

    fn read_var(&self, name: &Token, env: &Env, cx: &FnCx) -> Result<Typed, ExpandError> {
        match env.vars.get(&name.lexeme) {
            Some(v) => Ok((self.name_node(&v.cur, &name.span), self.resolved(&v.ty, cx))),
            None => err(format!("`{}` is not defined here", name.lexeme), &name.span),
        }
    }

    /// Bind `name` to `value`: a new name, or a rebinding under a fresh name.
    #[allow(clippy::too_many_arguments)]
    fn assign(&self, name: &Token, declared: Option<String>, value: Typed, shared: Option<String>, env: &mut Env, cx: &mut FnCx, b: &mut Vec<SchemaAstNode>) -> Result<(), ExpandError> {
        let (node, vty) = value;
        let vty = self.resolved(&vty, cx);
        let ty = declared.clone().or(vty.clone());
        let cur = match env.vars.get(&name.lexeme) {
            None => name.lexeme.clone(),
            Some(old) => {
                if let (Some(a), Some(n)) = (self.resolved(&old.ty, cx), ty.clone()) {
                    // `best = float("inf")` ... `best = x` (an int): in the
                    // discovery pass, note it; the real pass binds the
                    // sentinel as the largest / smallest int instead.
                    if cx.discover && old.sentinel.is_some() && a == self.spec.float_type && n == self.spec.types.int {
                        cx.int_sentinels.insert(name.lexeme.clone());
                    } else if !self.same_type(&a, &n, cx) {
                        return err(format!(
                            "`{}` holds {} here, but this assigns {}; a variable keeps one type — use a new name",
                            name.lexeme, self.spell(&a), self.spell(&n)
                        ), &name.span);
                    }
                }
                cx.fresh(&self.spec.reserved_prefix, name.lexeme.trim_start_matches(self.spec.reserved_prefix.as_str()))
            }
        };
        let ty = match (ty, env.vars.get(&name.lexeme)) {
            (Some(t), _) => Some(t),
            (None, Some(old)) => old.ty.clone(),
            (None, None) => None,
        };
        b.push(self.binding(&cur, node, &name.span));
        // Sharing is symmetric: after `ys = xs`, updating either one is visible
        // through the other in Python.
        if let Some(src) = &shared {
            if src != &name.lexeme {
                if let Some(v) = env.vars.get_mut(src) {
                    v.shared = Some(name.lexeme.clone());
                }
            }
        }
        env.vars.insert(name.lexeme.clone(), Var { ty, cur, param: false, shared, sentinel: None });
        Ok(())
    }

    /// Two types are the same, a pending element type (`list[?k]`,
    /// `dict[?k]`) unifying with a known one — the first-store rule.
    fn same_type(&self, a: &str, b: &str, cx: &mut FnCx) -> bool {
        if a == b {
            return true;
        }
        let (ga, gb) = match (Self::generic(a), Self::generic(b)) {
            (Some(x), Some(y)) => (x, y),
            _ => return false,
        };
        if ga.0 != gb.0 {
            return false;
        }
        let pending = |args: &[&str]| -> Option<String> {
            if args.len() == 1 { args[0].strip_prefix('?').map(String::from) } else { None }
        };
        for (p, q) in [(&ga.1, &gb.1), (&gb.1, &ga.1)] {
            if let Some(k) = pending(p) {
                if pending(q).is_some() {
                    return true;
                }
                if let Some(slot) = cx.pending.get_mut(&k) {
                    match slot {
                        None => {
                            *slot = Some(q.join(","));
                            return true;
                        }
                        Some(e) => {
                            let e = e.clone();
                            return self.same_type(&format!("{}[{}]", ga.0, e), &format!("{}[{}]", ga.0, q.join(",")), cx);
                        }
                    }
                }
            }
        }
        if ga.1.len() != gb.1.len() {
            return false;
        }
        let pairs: Vec<(String, String)> = ga.1.iter().zip(gb.1.iter()).map(|(x, y)| (x.to_string(), y.to_string())).collect();
        pairs.iter().all(|(x, y)| self.same_type(x, y, cx))
    }

    /// A natural update of `name` (list mutated in place in Python): refused
    /// where Python would show it through another name, else a rebinding.
    fn update(&self, name: &Token, value: Typed, env: &mut Env, cx: &mut FnCx, b: &mut Vec<SchemaAstNode>) -> Result<(), ExpandError> {
        let v = match env.vars.get(&name.lexeme) {
            Some(v) => v.clone(),
            None => return err(format!("`{}` is not defined here", name.lexeme), &name.span),
        };
        if v.param {
            return err(format!(
                "`{}` is a parameter: updating it would change the caller's value in Python; update a copy under a new name instead, e.g. `mine = {}.copy()`",
                name.lexeme, name.lexeme
            ), &name.span);
        }
        if let Some(other) = &v.shared {
            return err(format!(
                "`{}` may share its value with `{}`, so in Python this update would change both; make it a copy: `... = {}.copy()`",
                name.lexeme, other, other
            ), &name.span);
        }
        if env.iterating.contains(&name.lexeme) {
            return err(format!("`{}` is updated inside a loop over `{}`; loop over a copy: `for x in {}.copy():`", name.lexeme, name.lexeme, name.lexeme), &name.span);
        }
        self.assign(name, None, value, None, env, cx, b)
    }

    /// What a new binding would share with an existing list: `ys = xs`, or a
    /// list passed to (and possibly returned by) a same-file function.
    fn sharing(&self, value: &SchemaAstNode, env: &Env, cx: &FnCx) -> Option<String> {
        let v = self.strip_wrappers(value);
        let is_list_var = |n: &str| env.vars.get(n).and_then(|v| self.resolved(&v.ty, cx)).map_or(false, |t| Self::is_list(&t) || Self::is_dict(&t));
        match self.role(v) {
            "name" => {
                let n = &token(v, "name").ok()?.lexeme;
                if is_list_var(n) { Some(n.clone()) } else { None }
            }
            "postfix" => {
                let atom = self.strip_wrappers(node(v, "atom").ok()?);
                let trailers = nodes(v, "trailers");
                let first = trailers.first().map(|t| self.unwrap(t))?;
                if self.role(atom) == "name" && self.role(first) == "call_trailer" && trailers.len() == 1 {
                    let f = self.renamed(token(atom, "name").ok()?).lexeme;
                    if cx.sigs.contains_key(&f) {
                        let args = self.args_of(node(first, "args").ok()?).ok()?;
                        for a in args {
                            let a = self.strip_wrappers(a);
                            if self.role(a) == "name" {
                                let n = &token(a, "name").ok()?.lexeme;
                                if is_list_var(n) {
                                    return Some(n.clone());
                                }
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// `recv.m(...)` where `recv` is a name and `m` updates it in place.
    fn update_call(&self, e: &SchemaAstNode) -> Option<(Token, String)> {
        let e = self.strip_wrappers(e);
        if self.role(e) != "postfix" {
            return None;
        }
        let atom = self.strip_wrappers(node(e, "atom").ok()?);
        let trailers = nodes(e, "trailers");
        if self.role(atom) != "name" || trailers.len() != 1 {
            return None;
        }
        let t = self.unwrap(trailers[0]);
        if self.role(t) != "method_trailer" {
            return None;
        }
        let m = token(t, "name").ok()?.lexeme.clone();
        if ["append", "extend", "pop", "sort", "reverse", "insert", "update"].contains(&m.as_str()) {
            Some((token(atom, "name").ok()?.clone(), m))
        } else {
            None
        }
    }

    fn method_args<'a>(&self, e: &'a SchemaAstNode) -> Result<Vec<&'a SchemaAstNode>, ExpandError> {
        let e = self.strip_wrappers(e);
        let t = self.unwrap(nodes(e, "trailers")[0]);
        self.args_of(node(t, "args")?)
    }

    fn update_stmt(&self, recv: &Token, m: &str, e: &SchemaAstNode, env: &mut Env, cx: &mut FnCx, b: &mut Vec<SchemaAstNode>) -> Result<(), ExpandError> {
        let (xs, xty) = self.read_var(recv, env, cx)?;
        let xty = xty.map(|t| self.resolve(&t, cx)).unwrap_or_default();
        let args = self.method_args(e)?;
        if m == "update" {
            // `d.update(e)`: e's entries over d's, in place.
            if !Self::is_dict(&xty) || args.len() != 1 {
                return err(format!("`.update()` needs a dict and one dict argument; `{}` is {}", recv.lexeme, self.spell(&xty)), &recv.span);
            }
            let (other, oty) = self.expr(args[0], env, cx)?;
            if let Some(t) = &oty {
                if !self.same_type(&xty, t, cx) {
                    return err(format!("`{}` is {}; updating it with {} would change that", recv.lexeme, self.spell(&xty), self.spell(t)), &e.span);
                }
            }
            let new = self.instantiate(&self.spec.dicts.merge.clone(), &[xs, other], &e.span)?;
            return self.update(recv, (new, Some(xty)), env, cx, b);
        }
        if !Self::is_list(&xty) {
            return err(format!("`.{}()` needs a list; `{}` is {}", m, recv.lexeme, self.spell(&xty)), &recv.span);
        }
        let new = match m {
            "append" => {
                if args.len() != 1 {
                    return err("`.append()` takes one value", &e.span);
                }
                let (v, vty) = self.expr(args[0], env, cx)?;
                self.store_elem(&xty, &vty, cx, &e.span)?;
                self.instantiate(&self.spec.lists.append.clone(), &[xs, v], &e.span)?
            }
            "extend" => {
                if args.len() != 1 {
                    return err("`.extend()` takes one list", &e.span);
                }
                let (v, vty) = self.expr(args[0], env, cx)?;
                if let Some(vt) = &vty {
                    if let Some(ve) = Self::elem_of(vt) {
                        let ve = Some(ve.to_string());
                        self.store_elem(&xty, &ve, cx, &e.span)?;
                    }
                }
                self.instantiate(&self.spec.lists.extend.clone(), &[xs, v], &e.span)?
            }
            "pop" => {
                self.pop(recv, e, env, cx, b)?;
                return Ok(());
            }
            "reverse" => self.instantiate(&self.spec.slices.reverse_list.clone(), &[xs], &e.span)?,
            "insert" => {
                if args.len() != 2 {
                    return err("`.insert()` takes an index and a value", &e.span);
                }
                let i = self.int_expr(args[0], env, cx)?;
                let (v, vty) = self.expr(args[1], env, cx)?;
                self.store_elem(&xty, &vty, cx, &e.span)?;
                self.instantiate(&self.spec.lists.insert.clone(), &[xs, i, v], &e.span)?
            }
            "sort" => {
                // `xs.sort(reverse=...)` is `xs = sorted(xs, reverse=...)`.
                let t = self.unwrap(nodes(self.strip_wrappers(e), "trailers")[0]);
                let (pos, kws) = self.split_args(node(t, "args")?, env, cx)?;
                if !pos.is_empty() {
                    return err("`.sort()` takes only `reverse=`", &e.span);
                }
                self.sorted_value((xs, Some(xty.clone())), kws, &e.span, cx)?.0
            }
            _ => unreachable!(),
        };
        self.update(recv, (new, Some(xty)), env, cx, b)
    }

    /// `xs.pop()`: returns the last element, and updates `xs` to the rest.
    fn pop(&self, recv: &Token, e: &SchemaAstNode, env: &mut Env, cx: &mut FnCx, b: &mut Vec<SchemaAstNode>) -> Result<Typed, ExpandError> {
        let margs = self.method_args(e)?;
        if margs.len() > 1 {
            return err("`.pop()` takes at most one index", &e.span);
        }
        let idx = match margs.first() {
            Some(a) => Some(self.int_expr(a, env, cx)?),
            None => None,
        };
        let (xs, xty) = self.read_var(recv, env, cx)?;
        let xty = xty.map(|t| self.resolve(&t, cx)).unwrap_or_default();
        let ety = match Self::elem_of(&xty) {
            Some(t) if !t.starts_with('?') => t.to_string(),
            _ => return err(format!("`.pop()` needs to know what `{}` holds; annotate it, e.g. `{}: list[int] = []`", recv.lexeme, recv.lexeme), &e.span),
        };
        let (raw, rest) = match idx {
            None => (
                self.instantiate(&self.spec.lists.pop_value.clone(), &[xs.clone()], &e.span)?,
                self.instantiate(&self.spec.lists.pop_rest.clone(), &[xs], &e.span)?,
            ),
            Some(i) => {
                // `xs.pop(i)`: the index is read twice — bind it once.
                let k = cx.fresh(&self.spec.reserved_prefix, "i");
                b.push(self.binding(&k, i, &e.span));
                let kn = self.name_node(&k, &e.span);
                let at = self.normalise_index(&xs, kn.clone(), &e.span)?;
                (
                    self.instantiate(&self.spec.lists.get.clone(), &[xs.clone(), at], &e.span)?,
                    self.instantiate(&self.spec.lists.del.clone(), &[xs, kn], &e.span)?,
                )
            }
        };
        let v = self.unpack(&ety, raw, &e.span, cx)?;
        // The value is computed before the update, as Python evaluates it.
        let fresh = cx.fresh(&self.spec.reserved_prefix, "pop");
        b.push(self.binding(&fresh, v, &e.span));
        self.update(recv, (rest, Some(xty)), env, cx, b)?;
        Ok((self.name_node(&fresh, &e.span), Some(ety)))
    }

    /// The first-store rule: a store into a list whose element type is still
    /// pending fixes it; into a known one, it must agree.
    fn store_elem(&self, list_ty: &str, vty: &Option<String>, cx: &mut FnCx, span: &Span) -> Result<(), ExpandError> {
        let want = match Self::elem_of(list_ty) {
            Some(e) => e.to_string(),
            None => return Ok(()),
        };
        let got = match vty {
            Some(t) => t.clone(),
            None if cx.discover => return Ok(()),
            None => return err("cannot tell the type of the value stored here; annotate the variable it comes from", span),
        };
        let a = format!("list[{}]", want);
        let b = format!("list[{}]", got);
        if !self.same_type(&a, &b, cx) {
            return err(format!("this list holds {}; storing {} into it would change that", self.spell(&self.resolve(&want, cx)), self.spell(&got)), span);
        }
        Ok(())
    }

    /// `xs[i]` / `d[k]` / `t[0]` / `s[i]` read.
    fn subscript_read(&self, name: &Token, xty: &str, key: Typed, env: &Env, cx: &mut FnCx, span: &Span) -> Result<Typed, ExpandError> {
        let (xs, _) = self.read_var(name, env, cx)?;
        self.index_value((xs, Some(xty.to_string())), key, cx, span)
    }

    /// The new value of `xs` / `d` after `xs[i] = v` / `d[k] = v`.
    #[allow(clippy::too_many_arguments)]
    fn subscript_write(&self, name: &Token, xty: &str, key: Typed, value: Typed, env: &Env, cx: &mut FnCx, span: &Span) -> Result<Typed, ExpandError> {
        let (xs, _) = self.read_var(name, env, cx)?;
        let (k, kty) = key;
        let (v, vty) = value;
        if Self::is_list(xty) {
            if kty.as_deref() != Some(self.spec.types.int.as_str()) {
                return err("a list index must be an int", &k.span);
            }
            self.store_elem(xty, &vty, cx, span)?;
            let new = self.instantiate(&self.spec.lists.set.clone(), &[xs, k, v], span)?;
            return Ok((new, Some(xty.to_string())));
        }
        if Self::is_dict(xty) {
            let (kt, vt) = match (kty, vty) {
                (Some(a), Some(b)) => (a, b),
                // Discovery: a value typed by a later store (`counts[w] + 1`
                // before `counts[w] = 1`); carry on to reach that store.
                _ if cx.discover => return Ok((xs, Some(xty.to_string()))),
                _ => return err("cannot tell the type of the key or value stored here; annotate the variable it comes from", span),
            };
            let stored = format!("dict[{},{}]", kt, vt);
            if !self.same_type(xty, &stored, cx) {
                return err(format!("`{}` is {}; storing {} into it would change that", name.lexeme, self.spell(&self.resolve(xty, cx)), self.spell(&stored)), span);
            }
            let new = self.instantiate(&self.spec.dicts.set.clone(), &[xs, k, v], span)?;
            return Ok((new, Some(self.resolve(xty, cx))));
        }
        err(format!("`{}[...] = ...` needs a list or a dict; `{}` is {}", name.lexeme, name.lexeme, self.spell(xty)), &name.span)
    }

    /// Indexing a value of known type.
    fn index_value(&self, cur: Typed, key: Typed, cx: &mut FnCx, span: &Span) -> Result<Typed, ExpandError> {
        let ty = cur.1.clone().map(|t| self.resolve(&t, cx)).unwrap_or_default();
        let (k, kty) = key;
        let int = self.spec.types.int.clone();
        if ty.is_empty() || kty.is_none() {
            return self.soft(cx, "cannot tell the type of this index or of what it indexes; annotate the variable it comes from".into(), span);
        }
        if ty == self.spec.types.text || Self::is_list(&ty) {
            if kty.as_deref() != Some(int.as_str()) {
                return err(format!("an index must be {}, got {}", self.spell(&int), kty.as_deref().map(|t| self.spell(t)).unwrap_or("unknown".into())), &k.span);
            }
        }
        if ty == self.spec.types.text {
            let idx = self.normalise_index_text(&cur.0, k, span)?;
            return Ok((self.instantiate(&self.spec.text.get.clone(), &[cur.0, idx], span)?, Some(ty)));
        }
        if let Some(el) = Self::elem_of(&ty) {
            if el.starts_with('?') {
                return self.soft(cx, "cannot tell what this list holds yet; annotate it, e.g. `xs: list[int] = []`".into(), span);
            }
            let el = el.to_string();
            let idx = self.normalise_index(&cur.0, k, span)?;
            let raw = self.instantiate(&self.spec.lists.get.clone(), &[cur.0, idx], span)?;
            return Ok((self.unpack(&el, raw, span, cx)?, Some(el)));
        }
        if let Some(elems) = Self::tuple_elems(&ty) {
            let n = elems.len() as i64;
            let i = match int_value(&k, self.kind("int_lit")) {
                Some(i) if i >= -n && i < n => if i < 0 { i + n } else { i },
                _ => return err(format!("a tuple index must be a literal between {} and {}", -n, n - 1), &k.span),
            };
            let field = self.instantiate(&self.spec.loops.field.clone(), &[cur.0, int_lit(self.kind("int_lit"), i, span)], span)?;
            let et = elems[i as usize].clone();
            return Ok((self.unpack(&et, field, span, cx)?, Some(et)));
        }
        if Self::is_dict(&ty) {
            let (kt, vt) = match Self::dict_kv(&ty) {
                Some(kv) => kv,
                None => return self.soft(cx, "cannot tell what this dict holds yet; annotate it, e.g. `d: dict[str, int] = {}`".into(), span),
            };
            if let Some(got) = &kty {
                if !self.same_type(&kt, got, cx) {
                    return err(format!("this dict's keys are {}, not {}", self.spell(&kt), self.spell(got)), &k.span);
                }
            }
            let raw = self.instantiate(&self.spec.dicts.get.clone(), &[cur.0, k], span)?;
            return Ok((self.unpack(&vt, raw, span, cx)?, Some(vt)));
        }
        err(format!("`[...]` needs a list, a tuple, a dict or a text; this is {}", if ty.is_empty() { "of unknown type".to_string() } else { self.spell(&ty) }), span)
    }

    // ── loops ──────────────────────────────────────────────────────────────

    /// Names a statement list assigns (rebinds or updates) and names it reads.
    fn scan(&self, stmts: &[SchemaAstNode], assigned: &mut BTreeSet<String>, read: &mut BTreeSet<String>, has_return: &mut bool) {
        for st in stmts {
            match self.role(st) {
                // `xs[i] += v` and `del xs[i]` update xs too: missing here, xs was not carried through the loop's
                // state, so either statement in a loop body failed with "`xs` is not defined here".
                "assign" | "ann_assign" | "aug_assign" | "sub_assign" | "aug_sub_assign" | "del_stmt" => {
                    if let Ok(n) = token(st, "name") {
                        assigned.insert(n.lexeme.clone());
                        if self.role(st) != "assign" && self.role(st) != "ann_assign" {
                            read.insert(n.lexeme.clone());
                        }
                    }
                    if let Ok(v) = node(st, "value") {
                        if let Some((recv, _)) = self.update_call(v) {
                            assigned.insert(recv.lexeme.clone());
                        }
                    }
                }
                "expr_stmt" => {
                    if let Ok(e) = node(st, "expr") {
                        if let Some((recv, _)) = self.update_call(e) {
                            assigned.insert(recv.lexeme.clone());
                        }
                    }
                }
                "return" => *has_return = true,
                "if_stmt" | "elif" => {
                    if let Ok(b) = node(st, "then_branch") {
                        self.scan(&self.block_stmts(b), assigned, read, has_return);
                    }
                    if let Some(e) = nodes(st, "orelse").first() {
                        self.scan(&self.else_stmts(e), assigned, read, has_return);
                    }
                }
                "for_stmt" | "while_stmt" => {
                    if let Ok(b) = node(st, "body") {
                        self.scan(&self.block_stmts(b), assigned, read, has_return);
                    }
                }
                _ => {}
            }
            self.names_in(st, read);
        }
    }

    fn names_in(&self, n: &SchemaAstNode, out: &mut BTreeSet<String>) {
        if self.role(n) == "name" {
            if let Ok(t) = token(n, "name") {
                out.insert(t.lexeme.clone());
            }
        }
        for v in n.fields.values() {
            match v {
                SchemaValue::Node(c) => self.names_in(c, out),
                SchemaValue::Nodes(cs) => cs.iter().for_each(|c| self.names_in(c, out)),
                _ => {}
            }
        }
    }

    fn unpack(&self, ty: &str, raw: SchemaAstNode, span: &Span, cx: &FnCx) -> Result<SchemaAstNode, ExpandError> {
        let ty = self.resolve(ty, cx);
        let key = if Self::is_list(&ty) {
            "list[*]".to_string()
        } else if Self::is_tuple(&ty) {
            "tuple[*]".to_string()
        } else if Self::is_dict(&ty) {
            "dict[*]".to_string()
        } else {
            ty.clone()
        };
        match self.spec.loops.unpack.get(&key) {
            Some(t) => self.instantiate(&t.clone(), &[raw], span),
            None => err(format!("a {} cannot travel through loop state or a list", self.spell(&ty)), span),
        }
    }

    fn pack(&self, frame: &Frame, env: &Env, span: &Span, cx: &FnCx) -> Result<SchemaAstNode, ExpandError> {
        let mut vals = Vec::new();
        for (name, _) in &frame.state {
            let v = &env.vars[name];
            let _ = cx;
            vals.push(self.name_node(&v.cur, span));
        }
        self.instantiate(&self.spec.loops.pack.clone(), &vals, span)
    }

    /// Unpack every state slot into the lifted fn's env, as output bindings.
    fn unpack_state(&self, frame: &Frame, acc: &str, outer: &Env, env: &mut Env, span: &Span, cx: &mut FnCx) -> Result<Vec<SchemaAstNode>, ExpandError> {
        let mut b = Vec::new();
        for (k, (name, ty)) in frame.state.iter().enumerate() {
            let field = self.instantiate(&self.spec.loops.field.clone(), &[self.name_node(acc, span), int_lit(self.kind("int_lit"), k as i64, span)], span)?;
            let v = self.unpack(ty, field, span, cx)?;
            let cur = cx.fresh(&self.spec.reserved_prefix, name.trim_start_matches(&self.spec.reserved_prefix));
            b.push(self.binding(&cur, v, span));
            let (param, shared, sentinel) = outer.vars.get(name).map(|v| (v.param, v.shared.clone(), v.sentinel)).unwrap_or((false, None, None));
            env.vars.insert(name.clone(), Var { ty: Some(ty.clone()), cur, param, shared, sentinel });
        }
        Ok(b)
    }

    /// The loop's state: what the body updates and what it reads, that exists
    /// before it — in a stable order — plus the done flag and return slot when
    /// the body can `return`.
    fn frame_for(&self, body: &[SchemaAstNode], extra_reads: &[&SchemaAstNode], locals: &[String], env: &mut Env, cx: &mut FnCx, span: &Span) -> Result<(Frame, bool), ExpandError> {
        let mut assigned = BTreeSet::new();
        let mut read = BTreeSet::new();
        let mut has_return = false;
        self.scan(body, &mut assigned, &mut read, &mut has_return);
        for e in extra_reads {
            self.names_in(e, &mut read);
        }
        let mut state = Vec::new();
        for name in assigned.iter().chain(read.iter().filter(|r| !assigned.contains(*r))) {
            if locals.contains(name) || state.iter().any(|(n, _): &(String, String)| n == name) {
                continue;
            }
            if let Some(v) = env.vars.get(name) {
                let ty = match self.resolved(&v.ty, cx) {
                    Some(t) => t,
                    None if cx.discover => self.spec.types.value.clone(),
                    None => return err(format!("`{}` is used inside a loop, but its type is not written anywhere; annotate it", name), span),
                };
                state.push((name.clone(), ty));
            }
        }
        let p = &self.spec.reserved_prefix;
        if has_return {
            let ret = cx.ret.clone();
            let generic_key = if Self::is_list(&ret) { "list[*]" } else if Self::is_tuple(&ret) { "tuple[*]" } else if Self::is_dict(&ret) { "dict[*]" } else { "" };
            let default = match self.spec.loops.defaults.get(&ret).or_else(|| self.spec.loops.defaults.get(generic_key)) {
                Some(t) => self.instantiate(&t.clone(), &[], span)?,
                None => return err(format!("a `return` inside a loop is not supported in a function returning {}", self.spell(&ret)), span),
            };
            let done = format!("{}done", p);
            let rslot = format!("{}ret", p);
            if !env.vars.contains_key(&done) {
                // Introduce the slots in the enclosing scope (as bindings in the caller's block).
                env.pending_bindings.push((done.clone(), self.bool_lit(false, span), self.spec.types.bool.clone()));
                env.pending_bindings.push((rslot.clone(), default, ret.clone()));
            }
            state.push((done, self.spec.types.bool.clone()));
            state.push((rslot, ret));
        }
        Ok((Frame { state }, has_return))
    }

    fn flush_pending(&self, env: &mut Env, cx: &mut FnCx, b: &mut Vec<SchemaAstNode>, span: &Span) -> Result<(), ExpandError> {
        let pend: Vec<_> = std::mem::take(&mut env.pending_bindings);
        for (name, value, ty) in pend {
            self.assign(&ident(&name, span), Some(ty.clone()), (value, Some(ty)), None, env, cx, b)?;
        }
        Ok(())
    }

    /// A `for` / `while` statement. Returns `Some(result)` when the rest of the
    /// block was consumed (a loop that can `return` branches on its done flag).
    fn lower_loop(&self, st: &SchemaAstNode, rest: &[SchemaAstNode], env: &mut Env, cx: &mut FnCx, mode: &Mode, b: &mut Vec<SchemaAstNode>) -> Result<Option<SchemaAstNode>, ExpandError> {
        let span = st.span.clone();
        let body = self.block_stmts(node(st, "body")?);
        {
            let (mut assigned, mut read, mut hr) = (BTreeSet::new(), BTreeSet::new(), false);
            self.scan(&body, &mut assigned, &mut read, &mut hr);
            for n in assigned {
                if !env.vars.contains_key(&n) {
                    cx.scoped.entry(n).or_insert(Scoped::LoopBody);
                }
            }
            if let Ok(t) = node(st, "target") {
                for t in tokens(t, "names") {
                    cx.scoped.entry(t.lexeme.clone()).or_insert(Scoped::LoopVar);
                }
            }
        }
        let p = self.spec.reserved_prefix.clone();
        cx.counter += 1;
        let k = cx.counter;
        let lifted_name = format!("{}{}_loop{}", p, cx.fn_name, k);

        let (frame, has_return, st_value) = if self.role(st) == "for_stmt" {
            let targets: Vec<Token> = tokens(node(st, "target")?, "names");
            for t in &targets {
                self.reserved(t)?;
                if env.vars.contains_key(&t.lexeme) {
                    return err(format!("the loop variable `{}` already names something; use a new name", t.lexeme), &t.span);
                }
            }
            let iter_src = node(st, "iter")?;
            // Characters of a text expression: bind it to a name first, so
            // the loop can index it.
            let stripped = self.strip_wrappers(iter_src);
            let bound;
            let iter_src = if targets.len() == 1 && self.role(stripped) != "name" && self.simple_call(stripped).is_none() {
                let (v, ty) = self.expr(stripped, env, cx)?;
                let tmp = cx.fresh(&p, "s");
                b.push(self.binding(&tmp, v, &span));
                env.vars.insert(tmp.clone(), Var { ty, cur: tmp.clone(), param: false, shared: None, sentinel: None });
                bound = mk(self.kind("name"), &stripped.span, vec![("name", SchemaValue::Token(ident(&tmp, &stripped.span)))]);
                &bound
            } else {
                iter_src
            };
            let plan = self.iteration(&targets, iter_src, env, cx)?;
            let locals: Vec<String> = targets.iter().map(|t| t.lexeme.clone()).collect();
            let mut extra: Vec<&SchemaAstNode> = vec![];
            if let Some(src) = &plan.source {
                extra.push(src);
            }
            let (frame, has_return) = self.frame_for(&body, &extra, &locals, env, cx, &span)?;
            self.flush_pending(env, cx, b, &span)?;
            // The lifted step: (state, item) -> state.
            let acc = format!("{}acc", p);
            let item = format!("{}item", p);
            let mut lenv = Env::default();
            let mut pre = self.unpack_state(&frame, &acc, env, &mut lenv, &span, cx)?;
            plan.bind(&item, self, &mut lenv, cx, &mut pre, &span)?;
            if let Some(it) = &plan.iterating {
                lenv.iterating.insert(it.clone());
            }
            let lmode = Mode::Loop(Rc::new(frame.clone()));
            let lbody = self.loop_body(&body, lenv, cx, &lmode, pre, has_return, &acc, &span)?;
            let value = self.spec.types.value.clone();
            cx.lifted.push(self.fn_node(&lifted_name, &[(acc.clone(), value.clone()), (item, value.clone())], &value, lbody, &span));
            let init = self.pack(&frame, env, &span, cx)?;
            let fref = self.name_node(&lifted_name, &span);
            let v = self.instantiate(&self.spec.loops.fold.clone(), &[plan.list, init, fref], &span)?;
            (frame, has_return, v)
        } else {
            let cond_src = node(st, "condition")?;
            let (frame, has_return) = self.frame_for(&body, &[cond_src], &[], env, cx, &span)?;
            self.flush_pending(env, cx, b, &span)?;
            let acc = format!("{}acc", p);
            let value = self.spec.types.value.clone();
            // cond: state -> Bool
            let mut cenv = Env::default();
            let cpre = self.unpack_state(&frame, &acc, env, &mut cenv, &span, cx)?;
            let c = self.cond(cond_src, &mut cenv, cx)?;
            let cty = Some(self.spec.types.bool.clone());
            let c = if has_return {
                // Stop as soon as a `return` has happened.
                let done = self.name_node(&cenv.vars[&format!("{}done", p)].cur, &span);
                let not_done = self.dispatch_op("not", vec![(done, Some(self.spec.types.bool.clone()))], &span, cx)?;
                self.dispatch_op("and", vec![not_done, (c, cty)], &span, cx)?.0
            } else {
                c
            };
            let cond_name = format!("{}{}_cond{}", p, cx.fn_name, k);
            let cblock = self.finish(cpre, c, &span);
            cx.lifted.push(self.fn_node(&cond_name, &[(acc.clone(), value.clone())], &self.spec.types.bool.clone(), cblock, &span));
            // step: state -> state
            let mut senv = Env::default();
            let spre = self.unpack_state(&frame, &acc, env, &mut senv, &span, cx)?;
            let lmode = Mode::Loop(Rc::new(frame.clone()));
            let lbody = self.loop_body(&body, senv, cx, &lmode, spre, false, &acc, &span)?;
            cx.lifted.push(self.fn_node(&lifted_name, &[(acc, value.clone())], &value, lbody, &span));
            let init = self.pack(&frame, env, &span, cx)?;
            let bound = int_lit(self.kind("int_lit"), self.spec.loops.while_bound, &span);
            let v = self.instantiate(&self.spec.loops.while_.clone(), &[init, self.name_node(&cond_name, &span), self.name_node(&lifted_name, &span), bound], &span)?;
            // The bound ran out with the condition still true: fault, never
            // return the state as if the loop had finished.
            let stn = cx.fresh(&p, "st");
            b.push(self.binding(&stn, v, &span));
            let check = self.call_node(&cond_name, vec![self.name_node(&stn, &span)], &span);
            let fault = self.instantiate(&self.spec.loops.exhausted.clone(), &[], &span)?;
            let after = self.after_loop(&frame, has_return, &stn, rest, env.clone(), cx, mode, Vec::new(), &span)?;
            let guarded = self.if_node(check, self.finish(vec![], fault, &span), after, &span);
            return Ok(Some(guarded));
        };
        let stn = cx.fresh(&p, "st");
        b.push(self.binding(&stn, st_value, &span));
        if has_return {
            let after = self.after_loop(&frame, true, &stn, rest, env.clone(), cx, mode, Vec::new(), &span)?;
            return Ok(Some(after));
        }
        self.rebind_from_state(&frame, &stn, env, cx, b, &span)?;
        Ok(None)
    }

    /// The lifted body: unpacking, then (when the body can `return`) a skip
    /// once done, then the author's statements; falling off the end packs.
    #[allow(clippy::too_many_arguments)]
    fn loop_body(&self, body: &[SchemaAstNode], env: Env, cx: &mut FnCx, mode: &Mode, pre: Vec<SchemaAstNode>, skip_when_done: bool, acc: &str, span: &Span) -> Result<SchemaAstNode, ExpandError> {
        if skip_when_done {
            let done = self.name_node(&env.vars[&format!("{}done", self.spec.reserved_prefix)].cur, span);
            let skip = self.finish(vec![], self.name_node(acc, span), span);
            let run = self.block(body, env, cx, mode, Vec::new())?;
            return Ok(self.finish(pre, self.if_node(done, skip, run, span), span));
        }
        self.block(body, env, cx, mode, pre)
    }

    fn rebind_from_state(&self, frame: &Frame, stn: &str, env: &mut Env, cx: &mut FnCx, b: &mut Vec<SchemaAstNode>, span: &Span) -> Result<(), ExpandError> {
        for (k, (name, ty)) in frame.state.iter().enumerate() {
            let field = self.instantiate(&self.spec.loops.field.clone(), &[self.name_node(stn, span), int_lit(self.kind("int_lit"), k as i64, span)], span)?;
            let v = self.unpack(ty, field, span, cx)?;
            let cur = cx.fresh(&self.spec.reserved_prefix, name.trim_start_matches(&self.spec.reserved_prefix));
            b.push(self.binding(&cur, v, span));
            if let Some(var) = env.vars.get_mut(name) {
                var.cur = cur;
            }
        }
        Ok(())
    }

    /// After a loop that can `return`: rebind the state, then return the
    /// stored value if the done flag is set, else continue with `rest`.
    #[allow(clippy::too_many_arguments)]
    fn after_loop(&self, frame: &Frame, has_return: bool, stn: &str, rest: &[SchemaAstNode], mut env: Env, cx: &mut FnCx, mode: &Mode, mut b: Vec<SchemaAstNode>, span: &Span) -> Result<SchemaAstNode, ExpandError> {
        self.rebind_from_state(frame, stn, &mut env, cx, &mut b, span)?;
        if !has_return {
            return self.block(rest, env, cx, mode, b);
        }
        let p = &self.spec.reserved_prefix;
        let done = self.name_node(&env.vars[&format!("{}done", p)].cur, span);
        let rv = self.name_node(&env.vars[&format!("{}ret", p)].cur, span);
        let mut rb = Vec::new();
        let mut renv = env.clone();
        let ret_ty = Some(cx.ret.clone());
        let ret_result = self.do_return((rv, ret_ty), &mut renv, cx, mode, &mut rb, span)?;
        let then_b = self.finish(rb, ret_result, span);
        let else_b = self.block(rest, env, cx, mode, Vec::new())?;
        Ok(self.finish(b, self.if_node(done, then_b, else_b, span), span))
    }

    /// How a `for` iterates: the list folded over, and how each item binds
    /// the loop variable(s).
    fn iteration(&self, targets: &[Token], iter_src: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<IterPlan, ExpandError> {
        let span = iter_src.span.clone();
        let it = self.strip_wrappers(iter_src);
        // enumerate(xs) / zip(a, b) with two targets: iterate indices.
        if targets.len() == 2 {
            if let Some((f, args)) = self.simple_call(it) {
                let start = if f == "enumerate" && args.len() == 2 {
                    match int_value(self.strip_wrappers(args[1]), self.kind("int_lit")) {
                        Some(v) => Some(v),
                        None => return err("`enumerate(xs, start)` needs a literal start", &args[1].span),
                    }
                } else {
                    None
                };
                let args: Vec<&SchemaAstNode> = if start.is_some() { vec![args[0]] } else { args };
                if (f == "enumerate" && args.len() == 1) || (f == "zip" && args.len() == 2) {
                    let mut names = Vec::new();
                    for a in &args {
                        let a = self.strip_wrappers(a);
                        if self.role(a) != "name" {
                            return err(format!("this argument of `{}()` calls a function: binding it to a name for the loop would change when that call runs; assign it to a name on the line before", f), &a.span);
                        }
                        let t = token(a, "name")?.clone();
                        let (_, ty) = self.read_var(&t, env, cx)?;
                        let ty = ty.map(|t| self.resolve(&t, cx)).unwrap_or_default();
                        if f == "enumerate" && ty == self.spec.types.text {
                            let (sv, _) = self.read_var(&t, env, cx)?;
                            let n = self.instantiate(&self.spec.text.len.clone(), &[sv], &span)?;
                            let list = self.instantiate(&self.spec.loops.range.clone(), &[int_lit(self.kind("int_lit"), 0, &span), n], &span)?;
                            return Ok(IterPlan { list, kind: IterKind::EnumerateChars(t), targets: targets.to_vec(), source: Some(it.clone()), iterating: None });
                        }
                        if !Self::is_list(&ty) {
                            return err(format!("`{}()` needs a list; `{}` is {}", f, t.lexeme, self.spell(&ty)), &t.span);
                        }
                        names.push((t, ty));
                    }
                    let len = |n: &(Token, String), me: &Self, env: &Env, cx: &FnCx| -> Result<SchemaAstNode, ExpandError> {
                        let (x, _) = me.read_var(&n.0, env, cx)?;
                        me.instantiate(&me.spec.lists.len.clone(), &[x], &span)
                    };
                    let n = if f == "enumerate" {
                        len(&names[0], self, env, cx)?
                    } else {
                        let a = len(&names[0], self, env, cx)?;
                        let b = len(&names[1], self, env, cx)?;
                        self.dispatch_builtin("min", vec![(a, Some(self.spec.types.int.clone())), (b, Some(self.spec.types.int.clone()))], &span, cx)?.0
                    };
                    let list = self.instantiate(&self.spec.loops.range.clone(), &[int_lit(self.kind("int_lit"), 0, &span), n], &span)?;
                    let kind = if f == "enumerate" { IterKind::Enumerate(names[0].clone(), start.unwrap_or(0)) } else { IterKind::Zip(names[0].clone(), names[1].clone()) };
                    return Ok(IterPlan { list, kind, targets: targets.to_vec(), source: Some(it.clone()), iterating: None });
                }
            }
        }
        let (list, ty) = self.expr(it, env, cx)?;
        let ty = match ty {
            Some(t) => self.resolve(&t, cx),
            None if cx.discover => format!("list[{}]", self.spec.types.value),
            None => return err("cannot tell what this loop iterates over; annotate it", &span),
        };
        let ty = if cx.discover && ty.contains("[?") { format!("list[{}]", self.spec.types.value) } else { ty };
        // Over a dict: its keys.
        let (list, ty) = if Self::is_dict(&ty) {
            let (kt, _) = match Self::dict_kv(&ty) {
                Some(kv) => kv,
                None => return err("cannot tell what this dict holds yet; annotate it", &span),
            };
            (self.instantiate(&self.spec.dicts.keys.clone(), &[list], &span)?, format!("list[{}]", kt))
        } else {
            (list, ty)
        };
        if targets.len() > 1 {
            // Several names over a list of tuples: unpack each.
            return match Self::elem_of(&ty).and_then(Self::tuple_elems) {
                Some(parts) if parts.len() == targets.len() => Ok(IterPlan { list, kind: IterKind::Tuple(parts), targets: targets.to_vec(), source: None, iterating: None }),
                _ => err(format!("a {}-name `for` loop needs `enumerate(xs)`, `zip(a, b)` or a list of {}-tuples; this is {}", targets.len(), targets.len(), self.spell(&ty)), &span),
            };
        }
        let iterating = if self.role(it) == "name" { Some(token(it, "name")?.lexeme.clone()) } else { None };
        if ty == self.spec.types.text {
            // Characters of a text: iterate indices.
            if self.role(it) != "name" {
                return err("to loop over the characters of a text, assign it to a name first", &span);
            }
            let t = token(it, "name")?.clone();
            let n = self.instantiate(&self.spec.text.len.clone(), &[list], &span)?;
            let list = self.instantiate(&self.spec.loops.range.clone(), &[int_lit(self.kind("int_lit"), 0, &span), n], &span)?;
            return Ok(IterPlan { list, kind: IterKind::Chars(t), targets: targets.to_vec(), source: Some(it.clone()), iterating: None });
        }
        match Self::elem_of(&ty) {
            Some(e) if !e.starts_with('?') => Ok(IterPlan { list, kind: IterKind::List(e.to_string()), targets: targets.to_vec(), source: None, iterating }),
            Some(_) => err("cannot tell what this list holds yet; annotate it, e.g. `xs: list[int] = []`", &span),
            None => err(format!("a for loop needs a list, a range or a text; this is {}", self.spell(&ty)), &span),
        }
    }

    /// `f(args)` with a plain name callee.
    fn simple_call<'a>(&self, e: &'a SchemaAstNode) -> Option<(String, Vec<&'a SchemaAstNode>)> {
        if self.role(e) != "postfix" {
            return None;
        }
        let atom = self.strip_wrappers(node(e, "atom").ok()?);
        let trailers = nodes(e, "trailers");
        if self.role(atom) != "name" || trailers.len() != 1 {
            return None;
        }
        let t = self.unwrap(trailers[0]);
        if self.role(t) != "call_trailer" {
            return None;
        }
        let args = self.args_of(node(t, "args").ok()?).ok()?;
        Some((token(atom, "name").ok()?.lexeme.clone(), args))
    }

    // ── hoisting ───────────────────────────────────────────────────────────
    //
    // Some parts of an expression need a statement of their own: `xs.pop()`
    // (a value and an update) and a `zip` / `enumerate` argument the loop
    // must index by name. `hoist_stmt` rewrites the SOURCE statement into
    // `__h_N = <part>` statements plus the statement with names in their
    // place, so each piece goes through the ordinary, tested path.
    //
    // Exact only where Python evaluates the part exactly once, at the start
    // of the statement, so nothing is lifted out of a lazy position: a
    // ternary, the right of `and` / `or`, a later link of a comparison chain,
    // a comprehension's element or condition, a `while` condition or an
    // `assert` message. Moving it earlier must not change what anything else
    // in the statement sees, so:
    //  - `xs.pop()` is lifted only when `xs` appears nowhere else in the
    //    statement (Python would read it before the pop);
    //  - a comprehension's `zip` / `enumerate` argument is lifted only when
    //    it has no effect (no call but a builtin or a non-updating method);
    //    a `for` header's always is, as it is the statement's only
    //    expression and there is nothing to reorder.
    // Pops of different lists commute, so the order of the lifted parts is
    // free.

    /// The bindings to run first and the rewritten statement, or None when
    /// there is nothing to lift.
    fn hoist_stmt(&self, st: &SchemaAstNode, cx: &mut FnCx) -> Option<(Vec<SchemaAstNode>, SchemaAstNode)> {
        let role = self.role(st).to_string();
        let field = match role.as_str() {
            "assign" | "ann_assign" | "aug_assign" | "tuple_assign" => "value",
            "expr_stmt" | "return" => "expr",
            "if_stmt" | "elif" | "assert_stmt" => "condition",
            "for_stmt" => "iter",
            _ => return None,
        };
        let e = match st.fields.get(field) {
            Some(SchemaValue::Node(e)) => e.as_ref(),
            _ => return None,
        };
        // An update that IS the statement's value (`v = xs.pop()`,
        // `xs.append(v)`) already has its own path.
        if field != "condition" && field != "iter" && self.update_call(e).is_some() {
            return None;
        }
        // What the statement reads at this point: its own expression, and for
        // `x += ...` the target too. A branch or loop body runs later.
        let mut uses = BTreeMap::new();
        self.name_uses(e, &mut uses);
        if role == "aug_assign" {
            if let Ok(t) = token(st, "name") {
                *uses.entry(t.lexeme.clone()).or_insert(0) += 1;
            }
        }
        let mut pre = Vec::new();
        let new_e = if role == "for_stmt" {
            self.hoist_iter_args(e, true, cx, &mut pre)
        } else {
            self.hoist_expr(e, &uses, cx, &mut pre)
        };
        if pre.is_empty() {
            return None;
        }
        let mut out = st.clone();
        out.fields.insert(field.to_string(), SchemaValue::Node(Box::new(new_e)));
        Some((pre, out))
    }

    /// Every name read in `n`, with how many times it is read.
    fn name_uses(&self, n: &SchemaAstNode, out: &mut BTreeMap<String, usize>) {
        if self.role(n) == "name" {
            if let Ok(t) = token(n, "name") {
                *out.entry(t.lexeme.clone()).or_insert(0) += 1;
            }
        }
        for v in n.fields.values() {
            match v {
                SchemaValue::Node(c) => self.name_uses(c, out),
                SchemaValue::Nodes(cs) => cs.iter().for_each(|c| self.name_uses(c, out)),
                _ => {}
            }
        }
    }

    fn hoist_bind(&self, value: SchemaAstNode, cx: &mut FnCx, pre: &mut Vec<SchemaAstNode>) -> SchemaAstNode {
        let tmp = cx.fresh(&self.spec.reserved_prefix, "h");
        cx.hoisted.insert(tmp.clone());
        let span = value.span.clone();
        pre.push(self.binding(&tmp, value, &span));
        self.name_node(&tmp, &span)
    }

    /// `n` with its eagerly evaluated pops, and its comprehensions'
    /// `zip` / `enumerate` arguments, lifted into `pre`.
    fn hoist_expr(&self, n: &SchemaAstNode, uses: &BTreeMap<String, usize>, cx: &mut FnCx, pre: &mut Vec<SchemaAstNode>) -> SchemaAstNode {
        if let Some((recv, m)) = self.update_call(n) {
            if m == "pop" && uses.get(&recv.lexeme).copied().unwrap_or(0) == 1 {
                return self.hoist_bind(n.clone(), cx, pre);
            }
            return n.clone();
        }
        let role = self.role(n).to_string();
        let mut out = n.clone();
        let set = |out: &mut SchemaAstNode, f: &str, v: SchemaAstNode| {
            out.fields.insert(f.to_string(), SchemaValue::Node(Box::new(v)));
        };
        match role.as_str() {
            // Only a comprehension's iterable is evaluated in the enclosing
            // scope; its element and condition run once per item.
            "list_comp" | "gen_expr" | "dict_comp" => {
                if let Ok(it) = node(n, "iter") {
                    let it = self.hoist_iter_args(it, false, cx, pre);
                    let it = self.hoist_expr(&it, uses, cx, pre);
                    set(&mut out, "iter", it);
                }
                out
            }
            // `a if c else b`: every part is conditional.
            "cond_expr" if !nodes(n, "tail").is_empty() => out,
            // `a and b` / `a or b`: only `a` is unconditional.
            _ if n.kind == "OrExpr" || n.kind == "AndExpr" => {
                if let Ok(first) = node(n, "first") {
                    let first = self.hoist_expr(first, uses, cx, pre);
                    set(&mut out, "first", first);
                }
                out
            }
            // `a < b < c`: only the first link is unconditional.
            "compare" => {
                if let Ok(first) = node(n, "first") {
                    let first = self.hoist_expr(first, uses, cx, pre);
                    set(&mut out, "first", first);
                }
                let mut rest: Vec<SchemaAstNode> = nodes(n, "rest").iter().map(|r| (*r).clone()).collect();
                if let Some(r0) = rest.first_mut() {
                    if let Some(SchemaValue::Node(op)) = r0.fields.get("operand").cloned() {
                        let v = self.hoist_expr(&op, uses, cx, pre);
                        r0.fields.insert("operand".into(), SchemaValue::Node(Box::new(v)));
                    }
                    out.fields.insert("rest".into(), SchemaValue::Nodes(rest));
                }
                out
            }
            _ => {
                for (k, v) in n.fields.iter() {
                    match v {
                        SchemaValue::Node(c) => {
                            let c2 = self.hoist_expr(c, uses, cx, pre);
                            out.fields.insert(k.clone(), SchemaValue::Node(Box::new(c2)));
                        }
                        SchemaValue::Nodes(cs) => {
                            let cs2 = cs.iter().map(|c| self.hoist_expr(c, uses, cx, pre)).collect();
                            out.fields.insert(k.clone(), SchemaValue::Nodes(cs2));
                        }
                        _ => {}
                    }
                }
                out
            }
        }
    }

    /// `zip(a, b)` / `enumerate(a[, start])` as a loop's iterable: a list
    /// argument that is not a name is bound to one, since the loop reads it
    /// by index. `whole`: the call is a `for` header, the statement's only
    /// expression, so any argument may be lifted; inside a larger
    /// expression only an argument without effects may.
    fn hoist_iter_args(&self, it: &SchemaAstNode, whole: bool, cx: &mut FnCx, pre: &mut Vec<SchemaAstNode>) -> SchemaAstNode {
        let inner = self.strip_wrappers(it);
        let f = match self.simple_call(inner) {
            Some((f, _)) if f == "zip" || f == "enumerate" => f,
            _ => return it.clone(),
        };
        let lists = if f == "zip" { usize::MAX } else { 1 };
        let mut k = 0usize;
        let mut changed = false;
        let rewritten = self.map_call_args(inner, &mut |me: &Self, a: &SchemaAstNode| {
            let bare = me.strip_wrappers(a);
            let lift = k < lists && me.role(bare) != "name" && me.role(bare) != "kw_arg" && (whole || me.pure(bare));
            k += 1;
            if lift {
                changed = true;
                Some(me.hoist_bind(a.clone(), cx, pre))
            } else {
                None
            }
        });
        if changed { rewritten } else { it.clone() }
    }

    /// `call` (a one-trailer postfix call) with each positional argument
    /// passed through `f`; `Some(new)` replaces it.
    fn map_call_args(&self, call: &SchemaAstNode, f: &mut dyn FnMut(&Self, &SchemaAstNode) -> Option<SchemaAstNode>) -> SchemaAstNode {
        let mut out = call.clone();
        let mut trailers: Vec<SchemaAstNode> = nodes(call, "trailers").iter().map(|t| (*t).clone()).collect();
        let tr = self.rewrite_unwrapped(&trailers[0], &mut |t: &SchemaAstNode| {
            let mut t2 = t.clone();
            if let Some(SchemaValue::Node(al)) = t.fields.get("args") {
                let al2 = self.rewrite_unwrapped(al, &mut |list: &SchemaAstNode| {
                    let mut list2 = list.clone();
                    let args: Vec<SchemaAstNode> = nodes(list, "args").iter().map(|a| {
                        if a.kind == self.kind("empty") {
                            return (*a).clone();
                        }
                        if a.kind == self.kind("arg_rest") {
                            let mut a2 = (*a).clone();
                            if let Ok(v) = node(a, "arg") {
                                if let Some(n) = f(self, v) {
                                    a2.fields.insert("arg".into(), SchemaValue::Node(Box::new(n)));
                                }
                            }
                            return a2;
                        }
                        f(self, a).unwrap_or_else(|| (*a).clone())
                    }).collect();
                    list2.fields.insert("args".into(), SchemaValue::Nodes(args));
                    list2
                });
                t2.fields.insert("args".into(), SchemaValue::Node(Box::new(al2)));
            }
            t2
        });
        trailers[0] = tr;
        out.fields.insert("trailers".into(), SchemaValue::Nodes(trailers));
        out
    }

    /// Apply `f` to the node `unwrap` would return, keeping any transparent
    /// wrappers around it.
    fn rewrite_unwrapped(&self, n: &SchemaAstNode, f: &mut dyn FnMut(&SchemaAstNode) -> SchemaAstNode) -> SchemaAstNode {
        let target = self.unwrap(n);
        if std::ptr::eq(target, n) {
            return f(n);
        }
        let mut out = n.clone();
        if let Some(SchemaValue::Node(c)) = out.fields.get_mut("variant") {
            **c = self.rewrite_unwrapped(c, f);
            return out;
        }
        f(n)
    }

    /// No effect: every call is a builtin other than `print`, or a method
    /// that does not update its receiver.
    fn pure(&self, n: &SchemaAstNode) -> bool {
        if self.update_call(n).is_some() {
            return false;
        }
        if self.role(n) == "postfix" {
            let has_call = nodes(n, "trailers").iter().any(|t| self.role(self.unwrap(t)) == "call_trailer");
            if has_call {
                match self.simple_call(n) {
                    Some((f, _)) => {
                        let builtin = f != "print" && self.spec.builtins.keys().any(|k| k == &f || k.starts_with(&format!("{}(", f)));
                        if !builtin {
                            return false;
                        }
                    }
                    // `math.sqrt(x)` and the like are method-style calls,
                    // checked below; any other call shape is not provably pure.
                    None => {
                        let atom = node(n, "atom").map(|a| self.strip_wrappers(a));
                        let module = atom.ok().and_then(|a| token(a, "name").ok()).map_or(false, |t| self.spec.modules.contains(&t.lexeme));
                        if !module {
                            return false;
                        }
                    }
                }
            }
        }
        n.fields.values().all(|v| match v {
            SchemaValue::Node(c) => self.pure(c),
            SchemaValue::Nodes(cs) => cs.iter().all(|c| self.pure(c)),
            _ => true,
        })
    }

    // ── expressions ────────────────────────────────────────────────────────

    /// `expr` with a known expected type: an empty list literal takes it.
    fn expr_expect(&self, e: &SchemaAstNode, expected: Option<&str>, env: &mut Env, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let inner = self.strip_wrappers(e);
        if self.role(inner) == "dict_lit" && nodes(inner, "items").is_empty() {
            if let Some(t) = expected {
                if !Self::is_dict(t) {
                    return err(format!("`{{}}` is a dict, but {} is expected here", self.spell(t)), &inner.span);
                }
                let empty = self.instantiate(&self.spec.lists.new.clone(), &[], &inner.span)?;
                let d = self.instantiate(&self.spec.dicts.new.clone(), &[empty.clone(), empty], &inner.span)?;
                return Ok((d, Some(t.to_string())));
            }
        }
        if self.role(inner) == "list_lit" && self.args_of(node(inner, "args")?)?.is_empty() {
            let empty = self.instantiate(&self.spec.lists.new.clone(), &[], &inner.span)?;
            return match expected {
                Some(t) if Self::is_list(t) => Ok((empty, Some(t.to_string()))),
                Some(t) => err(format!("`[]` is a list, but {} is expected here", self.spell(t)), &inner.span),
                None => {
                    // First-store rule: the element type is fixed by the first store.
                    let id = cx.pend("l", &inner.span);
                    Ok((empty, Some(format!("list[?{}]", id))))
                }
            };
        }
        self.expr(e, env, cx)
    }

    /// Remove transparent wrappers, single-item chains and an empty cond tail.
    fn strip_wrappers<'a>(&self, mut e: &'a SchemaAstNode) -> &'a SchemaAstNode {
        loop {
            e = self.unwrap(e);
            let role = self.role(e);
            if self.spec.chains.contains(&e.kind) || role == "compare" {
                if nodes(e, "rest").is_empty() {
                    if let Ok(f) = node(e, "first") { e = f; continue; }
                }
            } else if role == "cond_expr" {
                if nodes(e, "tail").is_empty() {
                    if let Ok(v) = node(e, "value") { e = v; continue; }
                }
            } else if role == "postfix" {
                if nodes(e, "trailers").is_empty() {
                    if let Ok(a) = node(e, "atom") { e = a; continue; }
                }
            } else if role == "paren" {
                if let Ok(i) = node(e, "inner") { e = i; continue; }
            } else if role == "expr_list" {
                if nodes(e, "rest").is_empty() {
                    if let Ok(f) = node(e, "first") { e = f; continue; }
                }
            }
            return e;
        }
    }

    fn int_expr(&self, e: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<SchemaAstNode, ExpandError> {
        let (n, t) = self.expr(e, env, cx)?;
        match t {
            Some(t) if t == self.spec.types.int => Ok(n),
            Some(t) => err(format!("an index must be {}, got {}", self.spell(&self.spec.types.int), self.spell(&t)), &n.span),
            None if cx.discover => Ok(n),
            None => err("cannot tell the type of this index; annotate the variable it comes from", &n.span),
        }
    }

    /// Python's negative indices count from the end.
    fn normalise_index(&self, xs: &SchemaAstNode, idx: SchemaAstNode, span: &Span) -> Result<SchemaAstNode, ExpandError> {
        if let Some(v) = int_value(&idx, self.kind("int_lit")) {
            if v >= 0 {
                return Ok(idx);
            }
        }
        let len = self.instantiate(&self.spec.lists.len.clone(), &[xs.clone()], span)?;
        self.instantiate(&self.spec.lists.index.clone(), &[len, idx], span)
    }

    fn expr(&self, e: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let e = self.strip_wrappers(e);
        let t = self.spec.types.clone();
        if self.spec.chains.contains(&e.kind) {
            let mut acc_src = node(e, "first")?;
            let mut acc = self.expr(acc_src, env, cx)?;
            for item in nodes(e, "rest") {
                let op = token(item, "op")?;
                let rhs_src = node(item, "operand")?;
                let rhs = self.expr(rhs_src, env, cx)?;
                let (a2, rhs) = self.sentinel_pair(acc_src, acc, rhs_src, rhs, cx);
                acc = a2;
                // the running value is no longer an inf literal
                acc_src = item;
                acc = if op.lexeme == "and" || op.lexeme == "or" {
                    self.bool_op_value(&op.lexeme, acc, rhs, &op.span, cx)?
                } else {
                    self.dispatch_op(&op.lexeme, vec![acc, rhs], &op.span, cx)?
                };
            }
            return Ok(acc);
        }
        match self.role(e) {
            "compare" => self.compare(e, env, cx),
            "cond_expr" => {
                let value = self.expr(node(e, "value")?, env, cx)?;
                let tail = nodes(e, "tail")[0];
                let cond = self.cond(node(tail, "condition")?, env, cx)?;
                let orelse = self.expr(node(tail, "orelse")?, env, cx)?;
                // One type on both sides, as the verifier will demand; say so
                // here, at the author's line. Not even int beside float: in
                // Python the result keeps the type of the side that ran
                // (`n if n > 1 else 2.5` is an int when n > 1).
                if let (Some(a), Some(b)) = (self.resolved(&value.1, cx), self.resolved(&orelse.1, cx)) {
                    if !self.same_type(&a, &b, cx) && !cx.discover {
                        return err(format!("both sides of `... if ... else ...` must have one type; this one is {} on the left and {} on the right", self.spell(&a), self.spell(&b)), &e.span);
                    }
                }
                Ok((mk(self.kind("cond_expr"), &e.span, vec![
                    ("condition", SchemaValue::Node(Box::new(cond))),
                    ("then_branch", SchemaValue::Node(Box::new(value.0))),
                    ("orelse", SchemaValue::Node(Box::new(orelse.0))),
                ]), value.1.or(orelse.1)))
            }
            "not" => {
                let x = self.cond(node(e, "operand")?, env, cx)?;
                self.dispatch_op("not", vec![(x, Some(self.spec.types.bool.clone()))], &e.span, cx)
            }
            "neg" => {
                let operand = self.strip_wrappers(node(e, "operand")?);
                if operand.kind == self.kind("int_lit") {
                    let tok = token(operand, "value")?;
                    let neg = Token::new(tok.kind.clone(), format!("-{}", tok.lexeme), e.span.clone());
                    return Ok((mk(self.kind("int_lit"), &e.span, vec![("value", SchemaValue::Token(neg))]), Some(t.int.clone())));
                }
                let x = self.expr(operand, env, cx)?;
                self.dispatch_op("neg", vec![x], &e.span, cx)
            }
            "inv" => {
                // `~x` (INT_BIT_OPS_V1): one overload table entry, like `-x`
                let x = self.expr(node(e, "operand")?, env, cx)?;
                self.dispatch_op("~", vec![x], &e.span, cx)
            }
            "postfix" => self.postfix(e, env, cx),
            "name" => {
                let n = token(e, "name")?;
                if let Some(v) = env.vars.get(&n.lexeme) {
                    return Ok((self.name_node(&v.cur, &n.span), self.resolved(&v.ty, cx)));
                }
                if let Some((src, declared)) = cx.consts.get(&n.lexeme).cloned() {
                    if cx.const_stack.contains(&n.lexeme) {
                        return err(format!("the constant `{}` is defined through itself", n.lexeme), &n.span);
                    }
                    cx.const_stack.push(n.lexeme.clone());
                    let mut empty = Env::default();
                    let v = self.expr_expect(&src, declared.as_deref(), &mut empty, cx);
                    cx.const_stack.pop();
                    return v;
                }
                if let Some(scope) = cx.scoped.get(&n.lexeme) {
                    let x = &n.lexeme;
                    let why = match scope {
                        Scoped::LoopVar => format!("`{}` is a loop variable, and in PyAx it does not exist after its loop; assign what you need to a variable defined before the loop", x),
                        Scoped::LoopBody => format!("`{}` is first assigned inside a loop, and in PyAx it does not exist after the loop; assign it before the loop (e.g. `{} = 0`) so the loop updates it", x, x),
                        Scoped::Branch => format!("`{}` is assigned in only some branches of an `if` above; assign it before the `if`, or in every branch", x),
                    };
                    return err(why, &n.span);
                }
                let n = self.renamed(n);
                let ty = if cx.sigs.contains_key(&n.lexeme) { Some("Fn".to_string()) } else { None };
                Ok((mk(&e.kind, &e.span, vec![("name", SchemaValue::Token(n))]), ty))
            }
            "int_lit" => Ok((e.clone(), Some(t.int.clone()))),
            "float_lit" => {
                let lex = token(e, "value")?.lexeme.clone();
                let text = self.text_lit(&lex, &e.span);
                Ok((self.instantiate(&self.spec.float_literal.clone(), &[text], &e.span)?, Some(self.spec.float_type.clone())))
            }
            "text_lit" => Ok((e.clone(), Some(t.text.clone()))),
            "bool_lit" => {
                let tok = token(e, "value")?;
                let lex = self.spec.canonical_lexemes.get(&tok.lexeme).cloned().unwrap_or(tok.lexeme.clone());
                let canon = Token::new(tok.kind.clone(), lex, tok.span.clone());
                Ok((mk(&e.kind, &e.span, vec![("value", SchemaValue::Token(canon))]), Some(t.bool.clone())))
            }
            "none_lit" => Ok((e.clone(), Some(t.none.clone()))),
            "list_lit" => {
                let elems = self.args_of(node(e, "args")?)?;
                if elems.is_empty() {
                    return self.expr_expect(e, None, env, cx);
                }
                let mut typed = Vec::new();
                for el in elems {
                    typed.push(self.expr(el, env, cx)?);
                }
                let ety = match &typed[0].1 {
                    Some(ty) => ty.clone(),
                    None => return err("cannot tell the type of this list's first element; annotate the list", &typed[0].0.span),
                };
                for (n, ty) in &typed {
                    let same = match ty { Some(t) => self.same_type(&ety, t, cx), None => false };
                    if !same {
                        return err(format!("list elements must all be {}; this one is {}", self.spell(&ety), ty.as_deref().map(|t| self.spell(t)).unwrap_or("unknown".into())), &n.span);
                    }
                }
                let nodes: Vec<SchemaAstNode> = typed.into_iter().map(|t| t.0).collect();
                Ok((self.instantiate(&self.spec.lists.new.clone(), &nodes, &e.span)?, Some(format!("list[{}]", ety))))
            }
            "list_comp" | "gen_expr" | "dict_comp" => self.comprehension(e, env, cx),
            "expr_list" => {
                // `(a, b)` / `return a, b`: a tuple.
                let mut parts = vec![self.expr(node(e, "first")?, env, cx)?];
                for r in nodes(e, "rest") {
                    if let Some(item) = nodes(r, "item").first() {
                        parts.push(self.expr(item, env, cx)?);
                    }
                }
                let mut tys = Vec::new();
                for (n, t) in &parts {
                    match t {
                        Some(t) => tys.push(self.resolve(t, cx)),
                        None => return err("cannot tell the type of this tuple element; annotate the variable it comes from", &n.span),
                    }
                }
                let nodes: Vec<SchemaAstNode> = parts.into_iter().map(|p| p.0).collect();
                let v = self.instantiate(&self.spec.loops.pack.clone(), &nodes, &e.span)?;
                Ok((v, Some(format!("tuple[{}]", tys.join(",")))))
            }
            "dict_lit" => {
                let mut items: Vec<&SchemaAstNode> = Vec::new();
                for it in nodes(e, "items") {
                    if self.role(it) == "dict_rest" {
                        if let Some(x) = nodes(it, "item").first() {
                            items.push(x);
                        }
                    } else {
                        items.push(it);
                    }
                }
                // With `**other` entries the dict is built in order: merge each
                // spread, set each pair (later entries win, as in Python).
                let spread = |me: &Self, it: &SchemaAstNode| node(it, "key").map(|k| me.role(me.unwrap(k)) == "dict_spread").unwrap_or(false);
                if items.iter().any(|it| spread(self, it)) {
                    let empty = self.instantiate(&self.spec.lists.new.clone(), &[], &e.span)?;
                    let mut acc = self.instantiate(&self.spec.dicts.new.clone(), &[empty.clone(), empty], &e.span)?;
                    let mut ty: Option<String> = None;
                    for it in items {
                        if spread(self, it) {
                            let (d, dty) = self.expr(node(self.unwrap(node(it, "key")?), "value")?, env, cx)?;
                            if let Some(t) = &dty {
                                if !Self::is_dict(&self.resolve(t, cx)) {
                                    return err(format!("`**` in a dict needs a dict; this is {}", self.spell(t)), &d.span);
                                }
                            }
                            ty = ty.or(dty);
                            acc = self.instantiate(&self.spec.dicts.merge.clone(), &[acc, d], &e.span)?;
                        } else {
                            let (k, kty) = self.expr(node(it, "key")?, env, cx)?;
                            let (v, vty) = self.expr(nodes(it, "value").first().copied().ok_or_else(|| ExpandError { message: "dict entry without a value".into(), span: it.span.clone() })?, env, cx)?;
                            if let (Some(a), Some(b)) = (&kty, &vty) {
                                ty = ty.or(Some(format!("dict[{},{}]", a, b)));
                            }
                            acc = self.instantiate(&self.spec.dicts.set.clone(), &[acc, k, v], &e.span)?;
                        }
                    }
                    return Ok((acc, ty));
                }
                let (mut ks, mut vs) = (Vec::new(), Vec::new());
                let (mut kt, mut vt): (Option<String>, Option<String>) = (None, None);
                for it in items {
                    let (k, kty) = self.expr(node(it, "key")?, env, cx)?;
                    let (v, vty) = self.expr(nodes(it, "value").first().copied().ok_or_else(|| ExpandError { message: "dict entry without a value".into(), span: it.span.clone() })?, env, cx)?;
                    for (slot, got, n) in [(&mut kt, kty, &k), (&mut vt, vty, &v)] {
                        match (&*slot, got) {
                            (_, None) => return err("cannot tell the type of this dict entry; annotate the variable it comes from", &n.span),
                            (None, Some(g)) => *slot = Some(g),
                            (Some(a), Some(g)) if self.same_type(a, &g, cx) => {}
                            (Some(a), Some(g)) => return err(format!("dict entries must all be the same types; this one is {} where {} came before", self.spell(&g), self.spell(a)), &n.span),
                        }
                    }
                    ks.push(k);
                    vs.push(v);
                }
                let klist = self.instantiate(&self.spec.lists.new.clone(), &ks, &e.span)?;
                let vlist = self.instantiate(&self.spec.lists.new.clone(), &vs, &e.span)?;
                let d = self.instantiate(&self.spec.dicts.new.clone(), &[klist, vlist], &e.span)?;
                let ty = match (kt, vt) {
                    (Some(k), Some(v)) => format!("dict[{},{}]", k, v),
                    _ => {
                        // `{}`: the first-store rule fixes the key and value types.
                        let id = cx.pend("d", &e.span);
                        format!("dict[?{}]", id)
                    }
                };
                Ok((d, Some(ty)))
            }
            "fstring" => {
                let text = Some(self.spec.types.text.clone());
                let mut acc: Option<Typed> = None;
                for part in nodes(e, "parts") {
                    let part = self.unwrap(part);
                    let piece = if self.role(part) == "fexpr" {
                        let v = self.expr(node(part, "expr")?, env, cx)?;
                        self.dispatch_builtin("str", vec![v], &part.span, cx)?
                    } else {
                        (part.clone(), text.clone())
                    };
                    acc = Some(match acc {
                        None => piece,
                        Some(prev) => self.dispatch_op("+", vec![prev, piece], &part.span, cx)?,
                    });
                }
                match acc {
                    Some(a) => Ok(a),
                    None => {
                        let tok = Token::new(TokenKind::StringLit, "\"\"".to_string(), e.span.clone());
                        Ok((mk(self.kind("text_lit"), &e.span, vec![("value", SchemaValue::Token(tok))]), text))
                    }
                }
            }
            "" => err(format!("expression kind `{}` is not handled by expand", e.kind), &e.span),
            other => err(format!("`{}` is not an expression", other), &e.span),
        }
    }

    /// `[elem for x in xs if cond]` — a fold whose state is the result list
    /// plus what the comprehension reads.
    fn comprehension(&self, e: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let span = e.span.clone();
        let targets: Vec<Token> = tokens(node(e, "target")?, "names");
        for t in &targets {
            self.reserved(t)?;
            if env.vars.contains_key(&t.lexeme) {
                return err(format!("the comprehension variable `{}` already names something; use a new name", t.lexeme), &t.span);
            }
        }
        let plan = self.iteration(&targets, node(e, "iter")?, env, cx)?;
        let elem_src = node(e, "elem")?;
        let key_src = if self.role(e) == "dict_comp" { Some(node(e, "key")?) } else { None };
        let cond_src = nodes(e, "cond").first().map(|c| node(c, "condition")).transpose()?;
        let mut read = BTreeSet::new();
        self.names_in(elem_src, &mut read);
        if let Some(k) = key_src {
            self.names_in(k, &mut read);
        }
        if let Some(c) = cond_src {
            self.names_in(c, &mut read);
        }
        if let Some(s) = &plan.source {
            self.names_in(s, &mut read);
        }
        let p = self.spec.reserved_prefix.clone();
        let out = format!("{}out", p);
        let id = cx.pend("c", &span);
        let out_ty = if key_src.is_some() { format!("dict[?{}]", id) } else { format!("list[?{}]", id) };
        let mut state = vec![(out.clone(), out_ty.clone())];
        for name in &read {
            if targets.iter().any(|t| &t.lexeme == name) {
                continue;
            }
            if let Some(v) = env.vars.get(name) {
                match self.resolved(&v.ty, cx) {
                    Some(t) => state.push((name.clone(), t)),
                    None => return err(format!("`{}` is used in a comprehension, but its type is not written anywhere; annotate it", name), &span),
                }
            }
        }
        let frame = Frame { state };
        cx.counter += 1;
        let lifted_name = format!("{}{}_comp{}", p, cx.fn_name, cx.counter);
        let acc = format!("{}acc", p);
        let item = format!("{}item", p);
        // Inside: unpack, bind the target, maybe test, append.
        let mut lenv = Env::default();
        let outer_with_out = {
            let mut o = env.clone();
            o.vars.insert(out.clone(), Var { ty: Some(out_ty.clone()), cur: out.clone(), param: false, shared: None, sentinel: None });
            o
        };
        let mut pre = self.unpack_state(&frame, &acc, &outer_with_out, &mut lenv, &span, cx)?;
        plan.bind(&item, self, &mut lenv, cx, &mut pre, &span)?;
        let out_now = self.name_node(&lenv.vars[&out].cur, &span);
        let appended = match key_src {
            None => {
                let (el, ety) = self.expr(elem_src, &mut lenv, cx)?;
                self.store_elem(&out_ty, &ety, cx, &span)?;
                self.instantiate(&self.spec.lists.append.clone(), &[out_now, el], &span)?
            }
            Some(ks) => {
                // A dict comprehension: later keys overwrite earlier ones, as in Python.
                let (k, kty) = self.expr(ks, &mut lenv, cx)?;
                let (v, vty) = self.expr(elem_src, &mut lenv, cx)?;
                if let (Some(kt), Some(vt)) = (&kty, &vty) {
                    let seen = format!("dict[{},{}]", kt, vt);
                    self.same_type(&out_ty, &seen, cx);
                }
                self.instantiate(&self.spec.dicts.set.clone(), &[out_now, k, v], &span)?
            }
        };
        let packed_with = |me: &Self, out_value: SchemaAstNode, lenv: &Env| -> Result<SchemaAstNode, ExpandError> {
            let mut vals = vec![out_value];
            for (name, _) in frame.state.iter().skip(1) {
                vals.push(me.name_node(&lenv.vars[name].cur, &span));
            }
            me.instantiate(&me.spec.loops.pack.clone(), &vals, &span)
        };
        let result = match cond_src {
            None => packed_with(self, appended, &lenv)?,
            Some(c) => {
                let cn = self.cond(c, &mut lenv, cx)?;
                let keep = packed_with(self, appended, &lenv)?;
                let skip = packed_with(self, self.name_node(&lenv.vars[&out].cur, &span), &lenv)?;
                mk(self.kind("cond_expr"), &span, vec![
                    ("condition", SchemaValue::Node(Box::new(cn))),
                    ("then_branch", SchemaValue::Node(Box::new(keep))),
                    ("orelse", SchemaValue::Node(Box::new(skip))),
                ])
            }
        };
        let value = self.spec.types.value.clone();
        let lbody = self.finish(pre, result, &span);
        cx.lifted.push(self.fn_node(&lifted_name, &[(acc, value.clone()), (item, value.clone())], &value, lbody, &span));
        // Outside: fold, then read the list (or dict) back out of the state.
        let empty = self.instantiate(&self.spec.lists.new.clone(), &[], &span)?;
        let first = if key_src.is_some() {
            self.instantiate(&self.spec.dicts.new.clone(), &[empty.clone(), empty], &span)?
        } else {
            empty
        };
        let mut init_vals = vec![first];
        for (name, _) in frame.state.iter().skip(1) {
            init_vals.push(self.name_node(&env.vars[name].cur, &span));
        }
        let init = self.instantiate(&self.spec.loops.pack.clone(), &init_vals, &span)?;
        let folded = self.instantiate(&self.spec.loops.fold.clone(), &[plan.list, init, self.name_node(&lifted_name, &span)], &span)?;
        let field = self.instantiate(&self.spec.loops.field.clone(), &[folded, int_lit(self.kind("int_lit"), 0, &span)], &span)?;
        let list = self.unpack(&out_ty, field, &span, cx)?;
        Ok((list, Some(self.resolve(&out_ty, cx))))
    }

    fn args_of<'a>(&self, list: &'a SchemaAstNode) -> Result<Vec<&'a SchemaAstNode>, ExpandError> {
        let list = self.unwrap(list);
        if self.role(list) == "gen_expr" {
            return Ok(vec![list]);
        }
        let mut out = Vec::new();
        for a in nodes(list, "args") {
            if a.kind == self.kind("empty") {
                continue;
            }
            out.push(if a.kind == self.kind("arg_rest") { node(a, "arg")? } else { a });
        }
        Ok(out)
    }

    /// Python comparison chains: `a < b < c` is `a < b and b < c`, with `b`
    /// evaluated once. Duplicating `b` is only exact when `b` is a name or a
    /// literal, so anything else is refused rather than evaluated twice.
    /// An infinity literal (`float("-inf")`, `math.inf`) next to an int is
    /// the int sentinel, as an inf-initialised variable is (pyaxc's rule):
    /// `second == float("-inf")` must compare against the same value.
    fn sentinel_pair(&self, a_src: &SchemaAstNode, a: Typed, b_src: &SchemaAstNode, b: Typed, cx: &FnCx) -> (Typed, Typed) {
        let int = self.spec.types.int.clone();
        let as_int = |sign: i8, span: &Span| (int_lit(self.kind("int_lit"), if sign > 0 { i64::MAX } else { -i64::MAX }, span), Some(int.clone()));
        let ta = a.1.as_ref().map(|t| self.resolve(t, cx));
        let tb = b.1.as_ref().map(|t| self.resolve(t, cx));
        if tb.as_deref() == Some(int.as_str()) {
            if let Some(sign) = self.inf_sign(a_src) {
                let span = a.0.span.clone();
                return (as_int(sign, &span), b);
            }
        }
        if ta.as_deref() == Some(int.as_str()) {
            if let Some(sign) = self.inf_sign(b_src) {
                let span = b.0.span.clone();
                return (a, as_int(sign, &span));
            }
        }
        (a, b)
    }

    fn compare(&self, e: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let mut left_src = node(e, "first")?;
        let mut left = self.expr(left_src, env, cx)?;
        let rest = nodes(e, "rest");
        let mut acc: Option<Typed> = None;
        for (k, item) in rest.iter().enumerate() {
            let op_tok = token(item, "op")?;
            let op = if op_tok.lexeme == "not" { "not in".to_string() } else { op_tok.lexeme.clone() };
            let operand_src = node(item, "operand")?;
            let right = self.expr(operand_src, env, cx)?;
            let (l2, r2) = self.sentinel_pair(left_src, left, operand_src, right, cx);
            left = l2;
            let right = r2;
            left_src = operand_src;
            if k + 1 < rest.len() && !self.simple(&right.0) {
                return err("in a chained comparison the middle operand must be a name or a literal; split it: `a < b and b < c`", &right.0.span);
            }
            let cmp = self.dispatch_op(&op, vec![left, right.clone()], &op_tok.span, cx)?;
            acc = Some(match acc {
                None => cmp,
                Some(prev) => self.dispatch_op("and", vec![prev, cmp], &op_tok.span, cx)?,
            });
            left = right;
        }
        Ok(acc.expect("compare has at least one item when rest is non-empty"))
    }

    fn simple(&self, n: &SchemaAstNode) -> bool {
        let r = self.role(n);
        r == "name" || r.ends_with("_lit")
    }

    fn postfix(&self, e: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let atom = self.strip_wrappers(node(e, "atom")?);
        let trailers: Vec<&SchemaAstNode> = nodes(e, "trailers").into_iter().map(|t| self.unwrap(t)).collect();
        let mut i = 0;
        let mut cur: Typed;
        if self.role(atom) == "name" {
            let fname = token(atom, "name")?.clone();
            match trailers.first().map(|t| (self.role(t), *t)) {
                Some(("call_trailer", t)) => {
                    cur = self.call(&fname, node(t, "args")?, env, cx)?;
                    i = 1;
                }
                // `math.sqrt(x)`: a module function, dispatched like a builtin.
                Some(("method_trailer", t)) if self.spec.modules.contains(&fname.lexeme) && !env.vars.contains_key(&fname.lexeme) => {
                    let key = format!("{}.{}", fname.lexeme, token(t, "name")?.lexeme);
                    if !self.spec.builtins.contains_key(&key) {
                        return err(format!("`{}` is not in this language", key), &t.span);
                    }
                    let mut args = Vec::new();
                    for a in self.args_of(node(t, "args")?)? {
                        args.push(self.expr(a, env, cx)?);
                    }
                    cur = self.dispatch_builtin(&key, args, &e.span, cx)?;
                    i = 1;
                }
                // `math.pi`: a module constant.
                Some(("attr_trailer", t)) if self.spec.modules.contains(&fname.lexeme) && !env.vars.contains_key(&fname.lexeme) => {
                    let key = format!("{}.{}", fname.lexeme, token(t, "name")?.lexeme);
                    let c = match self.spec.module_constants.get(&key) {
                        Some(c) => c.clone(),
                        None => return err(format!("`{}` is not in this language", key), &t.span),
                    };
                    cur = (self.instantiate(&c.emit, &[], &e.span)?, Some(c.ret));
                    i = 1;
                }
                Some(("method_trailer", t)) if Some(&fname.lexeme) == self.spec.escape_namespace.as_ref() && !env.vars.contains_key(&fname.lexeme) => {
                    let callee = token(t, "name")?.clone();
                    let srcs = self.args_of(node(t, "args")?)?;
                    cur = match cx.reg {
                        Some(reg) if reg.contains_key(&callee.lexeme) => {
                            let (n, raw) = self.registry_call(&callee, &srcs, env, cx)?;
                            self.widen(n, &raw, &e.span)
                        }
                        Some(_) => return err(format!("`axis.{}` is not a registry function", callee.lexeme), &callee.span),
                        None => {
                            let mut args = Vec::new();
                            for a in srcs {
                                args.push(self.expr(a, env, cx)?.0);
                            }
                            (self.call_node(&callee.lexeme, args, &e.span), None)
                        }
                    };
                    i = 1;
                }
                _ => cur = self.expr(atom, env, cx)?,
            }
        } else {
            cur = self.expr(atom, env, cx)?;
        }
        for t in &trailers[i..] {
            match self.role(t) {
                "method_trailer" => {
                    let m = token(t, "name")?;
                    if ["append", "extend", "pop", "sort", "reverse", "insert", "update"].contains(&m.lexeme.as_str()) {
                        return err(if m.lexeme == "pop" {
                            "this `.pop()` can't be part of a larger expression here: the list is also used elsewhere in the statement, or the pop only runs on some paths (a branch of `... if ... else ...`, after `and` / `or`, a `while` condition, a comprehension body); write `v = xs.pop()` on its own line first".to_string()
                        } else {
                            format!("`.{}()` updates a list: write it as its own statement (`xs.{}(...)`)", m.lexeme, m.lexeme)
                        }, &m.span);
                    }
                    let recv_ty = match &cur.1 {
                        Some(ty) => self.resolve(ty, cx),
                        None => return err(format!("cannot tell the type of the value `.{}()` is called on; annotate it", m.lexeme), &cur.0.span),
                    };
                    if Self::is_dict(&recv_ty) {
                        cur = self.dict_method(cur, &recv_ty, m, node(t, "args")?, env, cx)?;
                        continue;
                    }
                    let mut args = vec![cur];
                    for a in self.args_of(node(t, "args")?)? {
                        args.push(self.expr(a, env, cx)?);
                    }
                    let key = format!("{}.{}", recv_ty, m.lexeme);
                    let list_key = format!("list[*].{}", m.lexeme);
                    let overloads = match self.spec.methods.get(&key).or_else(|| if Self::is_list(&recv_ty) { self.spec.methods.get(&list_key) } else { None }) {
                        Some(o) => o,
                        None => return err(format!("{} has no method `{}`", self.spell(&recv_ty), m.lexeme), &m.span),
                    };
                    cur = self.choose(&format!("`.{}()`", m.lexeme), overloads, args, &m.span, cx)?;
                }
                "index_trailer" => {
                    let key = self.expr(node(t, "index")?, env, cx)?;
                    cur = self.index_value(cur, key, cx, &t.span)?;
                }
                "slice_trailer" => {
                    let ty = cur.1.clone().map(|x| self.resolve(&x, cx)).unwrap_or_default();
                    let is_text = ty == self.spec.types.text;
                    if !is_text && !Self::is_list(&ty) {
                        return err(format!("slicing needs a list or a text; this is {}", if ty.is_empty() { "of unknown type".into() } else { self.spell(&ty) }), &t.span);
                    }
                    let bound = |me: &Self, n: &SchemaAstNode, env: &mut Env, cx: &mut FnCx| -> Result<Option<SchemaAstNode>, ExpandError> {
                        let n = me.unwrap(n);
                        if me.role(n) == "empty" {
                            return Ok(None);
                        }
                        Ok(Some(me.int_expr(n, env, cx)?))
                    };
                    let lo = bound(self, node(t, "lo")?, env, cx)?;
                    let hi = bound(self, node(t, "hi")?, env, cx)?;
                    let step = match nodes(t, "step").first() {
                        Some(st) => bound(self, node(st, "bound")?, env, cx)?,
                        None => None,
                    };
                    if let Some(step) = step {
                        if int_value(&step, self.kind("int_lit")) == Some(-1) && lo.is_none() && hi.is_none() {
                            let tmpl = if is_text { &self.spec.slices.reverse_text } else { &self.spec.slices.reverse_list };
                            cur = (self.instantiate(&tmpl.clone(), &[cur.0], &t.span)?, Some(ty));
                            continue;
                        }
                        let has = |b: &Option<SchemaAstNode>| self.bool_lit(b.is_some(), &t.span);
                        let (hl, hh) = (has(&lo), has(&hi));
                        let zero = || int_lit(self.kind("int_lit"), 0, &t.span);
                        let tmpl = if is_text { &self.spec.slices.step_text } else { &self.spec.slices.step_list };
                        cur = (self.instantiate(&tmpl.clone(), &[cur.0, lo.unwrap_or_else(zero), hi.unwrap_or_else(zero), step, hl, hh], &t.span)?, Some(ty));
                        continue;
                    }
                    let lo = lo.unwrap_or_else(|| int_lit(self.kind("int_lit"), 0, &t.span));
                    let hi = hi.unwrap_or_else(|| int_lit(self.kind("int_lit"), self.spec.slices.open_end, &t.span));
                    let tmpl = if is_text { &self.spec.slices.text } else { &self.spec.slices.list };
                    cur = (self.instantiate(&tmpl.clone(), &[cur.0, lo, hi], &t.span)?, Some(ty));
                }
                "call_trailer" => return err("only a named function can be called", &t.span),
                "attr_trailer" => return err("attributes are not in this language (only module constants such as `math.pi`)", &t.span),
                other => return err(format!("`{}` cannot follow an expression", other), &t.span),
            }
        }
        Ok(cur)
    }

    fn dict_method(&self, d: Typed, dty: &str, m: &Token, args: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let mut a = Vec::new();
        for x in self.args_of(args)? {
            a.push(self.expr(x, env, cx)?);
        }
        // `counts.get(w, 0)` before any store: the key and the default show
        // both types, so the first-store rule takes them from here.
        if Self::dict_kv(dty).is_none() && m.lexeme == "get" && a.len() == 2 {
            if let (Some(k), Some(v)) = (&a[0].1, &a[1].1) {
                let seen = format!("dict[{},{}]", self.resolve(k, cx), self.resolve(v, cx));
                self.same_type(dty, &seen, cx);
            }
        }
        let dty = self.resolve(dty, cx);
        let (kt, vt) = match Self::dict_kv(&dty) {
            Some(kv) => kv,
            None => return self.soft(cx, format!("cannot tell what this dict holds yet, so `.{}()` cannot be typed; annotate it, e.g. `d: dict[str, int] = {{}}`", m.lexeme), &m.span),
        };
        let span = &m.span;
        let r = &self.spec.dicts;
        match (m.lexeme.as_str(), a.len()) {
            ("get", 2) => {
                let (k, kty) = a.remove(0);
                let (dv, dty2) = a.remove(0);
                if let Some(t) = &kty {
                    if !self.same_type(&kt, t, cx) {
                        return err(format!("this dict's keys are {}, not {}", self.spell(&kt), self.spell(t)), &k.span);
                    }
                }
                if let Some(t) = &dty2 {
                    if !self.same_type(&vt, t, cx) {
                        return err(format!("this dict's values are {}; the default is {}", self.spell(&vt), self.spell(t)), &dv.span);
                    }
                }
                let raw = self.instantiate(&r.get_or.clone(), &[d.0, k, dv], span)?;
                Ok((self.unpack(&vt, raw, span, cx)?, Some(vt)))
            }
            ("get", 1) => err("`d.get(k)` without a default can return None, which is not in PyAx; give a default: `d.get(k, 0)`", span),
            ("keys", 0) => Ok((self.instantiate(&r.keys.clone(), &[d.0], span)?, Some(format!("list[{}]", kt)))),
            ("values", 0) => Ok((self.instantiate(&r.values.clone(), &[d.0], span)?, Some(format!("list[{}]", vt)))),
            ("items", 0) => Ok((self.instantiate(&r.items.clone(), &[d.0], span)?, Some(format!("list[tuple[{},{}]]", kt, vt)))),
            ("copy", 0) => Ok(d),
            (other, n) => err(format!("dicts have no method `{}` taking {} argument(s) in this language", other, n), span),
        }
    }

    fn normalise_index_text(&self, s: &SchemaAstNode, idx: SchemaAstNode, span: &Span) -> Result<SchemaAstNode, ExpandError> {
        if let Some(v) = int_value(&idx, self.kind("int_lit")) {
            if v >= 0 {
                return Ok(idx);
            }
        }
        let len = self.instantiate(&self.spec.text.len.clone(), &[s.clone()], span)?;
        self.instantiate(&self.spec.lists.index.clone(), &[len, idx], span)
    }

    /// `dict()`, `dict(d)` (a copy: dicts are values here), `dict(pairs)`
    /// over a list of 2-tuples, and `dict(zip(keys, values))`.
    fn dict_call(&self, fname: &Token, args: &[&SchemaAstNode], env: &mut Env, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let span = fname.span.clone();
        let usage = "dict() takes nothing, a dict, a list of (key, value) tuples, or zip(keys, values)";
        if let Some(k) = args.iter().find(|a| self.role(self.unwrap(a)) == "kw_arg") {
            return err("`dict(key=value)` is not in PyAx: write a literal, `{\"key\": value}`", &k.span);
        }
        match args {
            [] => {
                let lit = mk(self.kind("dict_lit"), &span, vec![("items", SchemaValue::Nodes(vec![]))]);
                self.expr_expect(&lit, None, env, cx)
            }
            [a] => {
                let a = self.strip_wrappers(a);
                if let Some((f, zargs)) = self.simple_call(a) {
                    if f == "zip" && zargs.len() == 2 && !env.vars.contains_key("zip") {
                        let (ks, kt) = self.expr(zargs[0], env, cx)?;
                        let (vs, vt) = self.expr(zargs[1], env, cx)?;
                        let elem = |t: &Option<String>, me: &Self, cx: &FnCx| t.as_ref().map(|t| me.resolve(t, cx)).filter(|t| Self::is_list(t)).and_then(|t| Self::elem_of(&t).map(String::from)).filter(|e| !e.starts_with('?'));
                        return match (elem(&kt, self, cx), elem(&vt, self, cx)) {
                            (Some(k), Some(v)) => Ok((self.call_node("py_dict_from_lists", vec![ks, vs], &span), Some(format!("dict[{},{}]", k, v)))),
                            _ => err("`dict(zip(keys, values))` needs two lists whose element types are known", &a.span),
                        };
                    }
                }
                let (v, ty) = self.expr(a, env, cx)?;
                let ty = ty.map(|t| self.resolve(&t, cx)).unwrap_or_default();
                if Self::is_dict(&ty) {
                    return Ok((v, Some(ty)));
                }
                match Self::elem_of(&ty).and_then(Self::tuple_elems) {
                    Some(parts) if parts.len() == 2 => Ok((self.call_node("py_dict_from_pairs", vec![v], &span), Some(format!("dict[{},{}]", parts[0], parts[1])))),
                    _ => err(format!("{}; this is {}", usage, self.spell(&ty)), &a.span),
                }
            }
            _ => err(usage, &span),
        }
    }

    fn call(&self, fname: &Token, arglist: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        // A local holding a function is not callable: only named functions are.
        if env.vars.contains_key(&fname.lexeme) {
            return err(format!("`{}` is a variable, not a function", fname.lexeme), &fname.span);
        }
        let fname = &self.renamed(fname);
        if let Some(msg) = self.spec.forbidden_calls.get(&fname.lexeme) {
            return err(msg.clone(), &fname.span);
        }
        let all = self.args_of(arglist)?;
        if let Some(sig) = cx.sigs.get(&fname.lexeme) {
            // A same-file def: positional, then keyword, then defaults.
            let names = sig.names.clone();
            let params = sig.params.clone();
            let defaults = sig.defaults.clone();
            let ret = sig.ret.clone();
            let mut slots: Vec<Option<SchemaAstNode>> = vec![None; names.len()];
            let mut k = 0;
            for a in &all {
                let a = self.unwrap(a);
                if self.role(a) == "kw_arg" {
                    let kn = token(a, "name")?;
                    let i = match names.iter().position(|n| n == &kn.lexeme) {
                        Some(i) => i,
                        None => return err(format!("`{}()` has no parameter `{}`", self.author_name(&fname.lexeme), kn.lexeme), &kn.span),
                    };
                    if slots[i].is_some() {
                        return err(format!("`{}` is given twice", kn.lexeme), &kn.span);
                    }
                    slots[i] = Some(self.expr_expect(node(a, "value")?, Some(&params[i]), env, cx)?.0);
                } else {
                    if k >= names.len() {
                        return err(format!("`{}()` takes {} argument(s)", self.author_name(&fname.lexeme), names.len()), &a.span);
                    }
                    slots[k] = Some(self.expr_expect(a, Some(&params[k]), env, cx)?.0);
                    k += 1;
                }
            }
            let mut nodes = Vec::new();
            for (i, slot) in slots.into_iter().enumerate() {
                nodes.push(match (slot, &defaults[i]) {
                    (Some(n), _) => n,
                    (None, Some(d)) => {
                        let mut empty = Env::default();
                        self.expr_expect(d, Some(&params[i]), &mut empty, cx)?.0
                    }
                    (None, None) => return err(format!("`{}()` is missing the argument `{}`", self.author_name(&fname.lexeme), names[i]), &fname.span),
                });
            }
            return Ok((self.call_node(&fname.lexeme, nodes, &fname.span), Some(ret)));
        }
        if fname.lexeme == "dict" {
            return self.dict_call(fname, &all, env, cx);
        }
        if all.iter().any(|a| self.role(self.unwrap(a)) == "kw_arg") && !cx.sigs.contains_key(&fname.lexeme) && self.spec.builtins.keys().any(|k| k.starts_with(&format!("{}(", fname.lexeme)) || self.spec.joined_builtins.contains_key(&fname.lexeme)) {
            let (pos, kws) = self.split_args(arglist, env, cx)?;
            return self.call_with_keywords(fname, pos, kws, cx);
        }
        if let Some(k) = all.iter().find(|a| self.role(self.unwrap(a)) == "kw_arg") {
            return err(format!("`{}()` takes no keyword arguments in this language", fname.lexeme), &k.span);
        }
        let srcs = all;
        // A same-file def wins over a builtin of the same name: the author's
        // own function is what a call to that name means.
        if let Some(sig) = cx.sigs.get(&fname.lexeme) {
            let params = sig.params.clone();
            let ret = sig.ret.clone();
            let mut nodes = Vec::new();
            for (k, a) in srcs.iter().enumerate() {
                let expected = params.get(k).map(String::as_str);
                nodes.push(self.expr_expect(a, expected, env, cx)?.0);
            }
            return Ok((self.call_node(&fname.lexeme, nodes, &fname.span), Some(ret)));
        }
        let builtin = self.spec.builtins.contains_key(&fname.lexeme)
            || self.spec.joined_builtins.contains_key(&fname.lexeme)
            || self.spec.builtins.keys().any(|k| k.starts_with(&format!("{}(", fname.lexeme)));
        if !builtin {
            if let Some(reg) = cx.reg {
                if !reg.contains_key(&fname.lexeme) {
                    return err(format!("`{}` is not defined: it is not a function in this file, a PyAx builtin, or a registry function", fname.lexeme), &fname.span);
                }
                let (n, raw) = self.registry_call(fname, &srcs, env, cx)?;
                return Ok(self.widen(n, &raw, &fname.span));
            }
        }
        let srcs_for_sentinel = srcs.clone();
        let mut args = Vec::new();
        for a in srcs {
            args.push(self.expr(a, env, cx)?);
        }
        if (fname.lexeme == "min" || fname.lexeme == "max") && args.len() == 2 {
            let b = args.pop().unwrap();
            let a = args.pop().unwrap();
            let (a, b) = self.sentinel_pair(srcs_for_sentinel[0], a, srcs_for_sentinel[1], b, cx);
            args = vec![a, b];
        }
        if self.spec.dict_args_as_keys.contains(&fname.lexeme) && args.len() == 1 {
            let t = args[0].1.clone().map(|t| self.resolve(&t, cx)).unwrap_or_default();
            if let Some((k, _)) = Self::dict_kv(&t) {
                let d = args.remove(0).0;
                let keys = self.instantiate(&self.spec.dicts.keys.clone(), &[d], &fname.span)?;
                args.push((keys, Some(format!("list[{}]", k))));
            }
        }
        if let Some(sep) = self.spec.joined_builtins.get(&fname.lexeme) {
            if args.len() > 1 {
                // `print(a, b)`: each part as text, joined.
                let sep_tok = Token::new(TokenKind::StringLit, format!("\"{}\"", sep), fname.span.clone());
                let sep_node = (mk(self.kind("text_lit"), &fname.span, vec![("value", SchemaValue::Token(sep_tok))]), Some(self.spec.types.text.clone()));
                let mut acc: Option<Typed> = None;
                for a in args {
                    let piece = self.dispatch_builtin("str", vec![a], &fname.span, cx)?;
                    acc = Some(match acc {
                        None => piece,
                        Some(prev) => {
                            let with_sep = self.dispatch_op("+", vec![prev, sep_node.clone()], &fname.span, cx)?;
                            self.dispatch_op("+", vec![with_sep, piece], &fname.span, cx)?
                        }
                    });
                }
                return self.dispatch_builtin(&fname.lexeme, vec![acc.unwrap()], &fname.span, cx);
            }
        }
        if fname.lexeme == "sorted" && args.len() == 1 {
            return self.sorted_value(args.remove(0), vec![], &fname.span, cx);
        }
        // min / max of a list of tuples: the first / last in tuple order.
        if (fname.lexeme == "min" || fname.lexeme == "max") && args.len() == 1 {
            let t = args[0].1.clone().map(|t| self.resolve(&t, cx)).unwrap_or_default();
            if let Some(et) = Self::elem_of(&t).filter(|e| Self::is_tuple(e)).map(String::from) {
                // max: sorted in reverse, first element (equal maxima are equal tuples)
                let rev = (ident("reverse", &fname.span), (self.bool_lit(fname.lexeme == "max", &fname.span), Some(self.spec.types.bool.clone())));
                let sorted = self.sorted_value(args.remove(0), vec![rev], &fname.span, cx)?;
                let at = int_lit(self.kind("int_lit"), 0, &fname.span);
                let raw = self.instantiate(&self.spec.lists.get.clone(), &[sorted.0, at], &fname.span)?;
                return Ok((self.unpack(&et, raw, &fname.span, cx)?, Some(et)));
            }
        }
        // `best = max(best, x)` with `best = -float("inf")` and an int `x`:
        // the sentinel is an int search's starting value (discovery notes it).
        if cx.discover && (fname.lexeme == "min" || fname.lexeme == "max") && args.len() == 2 {
            for (i, j) in [(0usize, 1usize), (1, 0)] {
                let a = self.strip_wrappers(srcs_for_sentinel[i]);
                if self.role(a) == "name" && args[j].1.as_deref() == Some(self.spec.types.int.as_str()) {
                    let n = token(a, "name")?.lexeme.clone();
                    if env.vars.get(&n).map_or(false, |v| v.sentinel.is_some()) {
                        cx.int_sentinels.insert(n);
                        return Ok((args.remove(j).0, Some(self.spec.types.int.clone())));
                    }
                }
            }
        }
        if self.spec.builtins.contains_key(&fname.lexeme) {
            return self.dispatch_builtin(&fname.lexeme, args, &fname.span, cx);
        }
        // Anything else is a registry function: called as written, result
        // type unknown here, checked by the verifier.
        let nodes = args.into_iter().map(|a| a.0).collect();
        Ok((self.call_node(&fname.lexeme, nodes, &fname.span), None))
    }

    // ── registry calls ─────────────────────────────────────────────────────
    //
    // A call to a registry function is typed from its registry signature,
    // as lowering resolves it on every surface. PyAx's `list[T]` is a
    // ValueList; a registry IntList / TextList / BoolList parameter gets the
    // list narrowed (value_list_to_*_list), and such a result is widened back
    // (pyax_*_list_widen). Both are runtime identities that check the
    // elements. A `Value` / `ValueList` result stays untyped: what it holds
    // is the author's to say, with an annotation, as before.

    /// The typed-list registry type for a PyAx element type, with its narrow
    /// and widen callables.
    fn typed_list(axis_ty: &str) -> Option<(&'static str, &'static str, &'static str)> {
        match axis_ty {
            "IntList" => Some(("Int", "value_list_to_int_list", "pyax_int_list_widen")),
            "TextList" => Some(("Text", "value_list_to_text_list", "pyax_text_list_widen")),
            "BoolList" => Some(("Bool", "value_list_to_bool_list", "pyax_bool_list_widen")),
            _ => None,
        }
    }

    fn spell_reg(&self, axis_ty: &str) -> String {
        match Self::typed_list(axis_ty) {
            Some((e, _, _)) => format!("list[{}]", self.spell(e)),
            None => self.spell(axis_ty),
        }
    }

    /// A registry call's result as a PyAx value: `raw` is its registry type.
    fn widen(&self, n: SchemaAstNode, raw: &str, span: &Span) -> Typed {
        if let Some((elem, _, widen)) = Self::typed_list(raw) {
            return (self.call_node(widen, vec![n], span), Some(format!("list[{}]", elem)));
        }
        if raw == self.spec.types.value || raw == self.spec.types.list_axis {
            return (n, None);
        }
        (n, Some(raw.to_string()))
    }

    fn is_registry_call(&self, e: &SchemaAstNode, env: &Env, cx: &FnCx) -> Option<String> {
        let (f, _) = self.simple_call(self.strip_wrappers(e))?;
        let reg = cx.reg?;
        let builtin = self.spec.builtins.contains_key(&f) || self.spec.builtins.keys().any(|k| k.starts_with(&format!("{}(", f)));
        (reg.contains_key(&f) && !cx.sigs.contains_key(&f) && !builtin && !env.vars.contains_key(&f)).then_some(f)
    }

    /// The call node and the registry's result type. Arity and the argument
    /// types expand can see are checked here, at the author's line; the
    /// verifier checks the rest, as on every surface.
    fn registry_call(&self, fname: &Token, srcs: &[&SchemaAstNode], env: &mut Env, cx: &mut FnCx) -> Result<(SchemaAstNode, String), ExpandError> {
        let sig = cx.reg.and_then(|r| r.get(&fname.lexeme)).cloned().expect("registry_call: name checked by the caller");
        let shown = || format!("({}) -> {}", sig.params.iter().map(|p| self.spell_reg(p)).collect::<Vec<_>>().join(", "), self.spell_reg(&sig.ret));
        if let Some(k) = srcs.iter().find(|a| self.role(self.unwrap(a)) == "kw_arg") {
            return err(format!("`{}()` is a registry function and takes no keyword arguments", fname.lexeme), &k.span);
        }
        if !sig.variadic && srcs.len() != sig.params.len() {
            return err(format!("`{}()` takes {} argument(s), {}; this call passes {}", fname.lexeme, sig.params.len(), shown(), srcs.len()), &fname.span);
        }
        let mut nodes = Vec::new();
        for (k, a) in srcs.iter().enumerate() {
            let param = if sig.variadic {
                if sig.params.len() == 1 { sig.params.first().cloned() } else { None }
            } else {
                sig.params.get(k).cloned()
            };
            // A registry call whose raw result already is this parameter's
            // type is passed as is (no widen-then-narrow).
            if let Some(p) = param.as_deref().filter(|p| Self::typed_list(p).is_some()) {
                if let Some(g) = self.is_registry_call(a, env, cx) {
                    let inner = self.strip_wrappers(a);
                    let gargs = match self.simple_call(inner) { Some((_, xs)) => xs, None => vec![] };
                    let (n, raw) = self.registry_call(&ident(&g, &inner.span), &gargs, env, cx)?;
                    if raw == p {
                        nodes.push(n);
                        continue;
                    }
                    let (v, ty) = self.widen(n, &raw, &inner.span);
                    nodes.push(self.registry_arg(fname, k, p, v, ty, &a.span, cx)?);
                    continue;
                }
            }
            let (v, ty) = self.expr(a, env, cx)?;
            nodes.push(match param.as_deref() {
                Some(p) => self.registry_arg(fname, k, p, v, ty, &a.span, cx)?,
                None => v,
            });
        }
        Ok((self.call_node(&fname.lexeme, nodes, &fname.span), sig.ret.clone()))
    }

    #[allow(clippy::too_many_arguments)]
    fn registry_arg(&self, fname: &Token, k: usize, p: &str, v: SchemaAstNode, ty: Option<String>, span: &Span, cx: &mut FnCx) -> Result<SchemaAstNode, ExpandError> {
        let ty = self.resolved(&ty, cx);
        let wrong = |got: &str| err(format!("argument {} of `{}()` must be {}; this is {}", k + 1, fname.lexeme, self.spell_reg(p), self.spell(got)), span);
        if let Some((elem, narrow, _)) = Self::typed_list(p) {
            return match ty {
                Some(t) if Self::is_list(&t) => match Self::elem_of(&t) {
                    Some(e) if e == elem => Ok(self.call_node(narrow, vec![v], span)),
                    Some(e) if e.starts_with('?') => {
                        // An empty `[]` passed here takes the element type.
                        let want = format!("list[{}]", elem);
                        if self.same_type(&t, &want, cx) { Ok(self.call_node(narrow, vec![v], span)) } else { wrong(&t) }
                    }
                    _ => wrong(&t),
                },
                Some(t) if t == p => Ok(v),
                Some(t) if !t.starts_with('?') && t != self.spec.types.value => wrong(&t),
                _ => Ok(v),
            };
        }
        let t = &self.spec.types;
        let primitive = [&t.int, &t.text, &t.bool, &t.none, &self.spec.float_type].iter().any(|x| x.as_str() == p) || p == "Dec";
        if primitive {
            if let Some(got) = ty.as_deref() {
                let known = !got.starts_with('?') && got != t.value && !got.contains("[?");
                if known && got != p {
                    return wrong(got);
                }
            }
        }
        Ok(v)
    }

    /// Positional arguments and keyword arguments (by name), expanded.
    fn split_args(&self, arglist: &SchemaAstNode, env: &mut Env, cx: &mut FnCx) -> Result<(Vec<Typed>, Vec<(Token, Typed)>), ExpandError> {
        let mut pos = Vec::new();
        let mut kws: Vec<(Token, Typed)> = Vec::new();
        for a in self.args_of(arglist)? {
            let a = self.unwrap(a);
            if self.role(a) == "kw_arg" {
                let k = token(a, "name")?.clone();
                if kws.iter().any(|(n, _)| n.lexeme == k.lexeme) {
                    return err(format!("`{}=` is given twice", k.lexeme), &k.span);
                }
                let v = self.expr(node(a, "value")?, env, cx)?;
                kws.push((k, v));
            } else {
                if !kws.is_empty() {
                    return err("a positional argument cannot follow a keyword argument", &a.span);
                }
                pos.push(self.expr(a, env, cx)?);
            }
        }
        kws.sort_by(|x, y| x.0.lexeme.cmp(&y.0.lexeme));
        Ok((pos, kws))
    }

    /// `f(a, k=v)`: the builtin entry `f(k)` with arguments `a, v`; `print`'s
    /// `sep=` is its join separator.
    fn call_with_keywords(&self, fname: &Token, mut pos: Vec<Typed>, mut kws: Vec<(Token, Typed)>, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        if fname.lexeme == "sorted" && pos.len() == 1 {
            let x = pos.remove(0);
            return self.sorted_value(x, kws, &fname.span, cx);
        }
        if let Some(default_sep) = self.spec.joined_builtins.get(&fname.lexeme).cloned() {
            let sep = match kws.iter().position(|(k, _)| k.lexeme == "sep") {
                Some(i) => kws.remove(i).1,
                None => (self.text_lit(&default_sep, &fname.span), Some(self.spec.types.text.clone())),
            };
            if pos.len() > 1 || !kws.is_empty() {
                let mut acc: Option<Typed> = None;
                for a in pos.drain(..) {
                    let piece = self.dispatch_builtin("str", vec![a], &fname.span, cx)?;
                    acc = Some(match acc {
                        None => piece,
                        Some(prev) => {
                            let with_sep = self.dispatch_op("+", vec![prev, sep.clone()], &fname.span, cx)?;
                            self.dispatch_op("+", vec![with_sep, piece], &fname.span, cx)?
                        }
                    });
                }
                if let Some(a) = acc {
                    pos.push(a);
                }
            }
        }
        if kws.is_empty() {
            return self.dispatch_builtin(&fname.lexeme, pos, &fname.span, cx);
        }
        let key = format!("{}({})", fname.lexeme, kws.iter().map(|(k, _)| k.lexeme.clone()).collect::<Vec<_>>().join(","));
        if !self.spec.builtins.contains_key(&key) {
            let names: Vec<String> = kws.iter().map(|(k, _)| format!("`{}=`", k.lexeme)).collect();
            return err(format!("`{}()` does not take {} in this language", fname.lexeme, names.join(", ")), &kws[0].0.span);
        }
        pos.extend(kws.into_iter().map(|(_, v)| v));
        self.dispatch_builtin(&key, pos, &fname.span, cx)
    }

    /// `sorted(xs)` / `sorted(xs, reverse=r)` (also `xs.sort(...)`). A list of
    /// tuples is ordered through order-preserving text keys (`sort_keys`).
    fn sorted_value(&self, xs: Typed, kws: Vec<(Token, Typed)>, span: &Span, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let mut reverse: Option<Typed> = None;
        for (k, v) in kws {
            match k.lexeme.as_str() {
                "reverse" => reverse = Some(v),
                "key" => return err("`key=` needs a lambda, which is not in PyAx; sort a list of tuples whose first part is the key instead", &k.span),
                other => return err(format!("`sorted()` does not take `{}=`", other), &k.span),
            }
        }
        let ty = xs.1.clone().map(|t| self.resolve(&t, cx)).unwrap_or_default();
        let ty = if Self::is_dict(&ty) { return err("sort a dict's keys with `sorted(d.keys())`", span) } else { ty };
        if let Some(parts) = Self::elem_of(&ty).and_then(Self::tuple_elems) {
            let rev = match reverse {
                Some(r) => r.0,
                None => self.bool_lit(false, span),
            };
            return self.sort_tuples(xs, &parts, rev, span, cx);
        }
        match reverse {
            None => self.dispatch_builtin("sorted", vec![xs], span, cx),
            Some(r) => self.dispatch_builtin("sorted(reverse)", vec![xs, r], span, cx),
        }
    }

    /// Python's repr() of a value of known type, as Text.
    fn repr_value(&self, v: Typed, span: &Span, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let text = Some(self.spec.types.text.clone());
        let ty = match &v.1 {
            Some(t) => self.resolve(t, cx),
            None => return self.soft(cx, "cannot tell the type of this value to print it; annotate it".into(), span),
        };
        let lit = |s: &str| (self.text_lit(s, span), text.clone());
        let cat = |me: &Self, a: Typed, b: Typed, cx: &mut FnCx| me.dispatch_op("+", vec![a, b], span, cx);
        if let Some(parts) = Self::tuple_elems(&ty) {
            if !self.simple(&v.0) {
                // Each part is read separately: lift `repr(t)` into a fn of
                // the tuple, so the expression is evaluated once.
                let p = format!("{}t", self.spec.reserved_prefix);
                let body = self.repr_value((self.name_node(&p, span), Some(ty.clone())), span, cx)?;
                cx.counter += 1;
                let name = format!("{}{}_reprt{}", self.spec.reserved_prefix, cx.fn_name, cx.counter);
                cx.lifted.push(self.fn_node(&name, &[(p, self.spec.types.value.clone())], &self.spec.types.text.clone(), self.finish(vec![], body.0, span), span));
                return Ok((self.call_node(&name, vec![v.0], span), text));
            }
            let mut acc = lit("(");
            for (i, t) in parts.iter().enumerate() {
                if i > 0 {
                    acc = cat(self, acc, lit(", "), cx)?;
                }
                let field = self.instantiate(&self.spec.loops.field.clone(), &[v.0.clone(), int_lit(self.kind("int_lit"), i as i64, span)], span)?;
                let part = self.repr_value((self.unpack(t, field, span, cx)?, Some(t.clone())), span, cx)?;
                acc = cat(self, acc, part, cx)?;
            }
            let close = if parts.len() == 1 { ",)" } else { ")" };
            return cat(self, acc, lit(close), cx);
        }
        let (list, elem, open, close, kv) = if let Some(e) = Self::elem_of(&ty) {
            if e.starts_with('?') {
                return self.soft(cx, "cannot tell what this list holds yet; annotate it".into(), span);
            }
            (v.0, e.to_string(), "[", "]", false)
        } else if let Some((k, val)) = Self::dict_kv(&ty) {
            let items = self.instantiate(&self.spec.dicts.items.clone(), &[v.0], span)?;
            (items, format!("tuple[{},{}]", k, val), "{", "}", true)
        } else {
            return match self.spec.repr.get(&ty) {
                Some(t) => Ok((self.instantiate(&t.clone(), &[v.0], span)?, text)),
                None => err(format!("{} cannot be printed in this language", self.spell(&ty)), span),
            };
        };
        // A lifted fold collecting each element's repr, then joined.
        let p = self.spec.reserved_prefix.clone();
        let (acc, item) = (format!("{}acc", p), format!("{}item", p));
        let el = (self.unpack(&elem, self.name_node(&item, span), span, cx)?, Some(elem.clone()));
        let piece = if kv {
            let parts = Self::tuple_elems(&elem).unwrap_or_default();
            let mut pieces = Vec::new();
            for (i, t) in parts.iter().enumerate() {
                let field = self.instantiate(&self.spec.loops.field.clone(), &[self.name_node(&item, span), int_lit(self.kind("int_lit"), i as i64, span)], span)?;
                pieces.push(self.repr_value((self.unpack(t, field, span, cx)?, Some(t.clone())), span, cx)?);
            }
            let kc = cat(self, pieces.remove(0), lit(": "), cx)?;
            cat(self, kc, pieces.remove(0), cx)?
        } else {
            self.repr_value(el, span, cx)?
        };
        let out_field = self.instantiate(&self.spec.loops.field.clone(), &[self.name_node(&acc, span), int_lit(self.kind("int_lit"), 0, span)], span)?;
        let out = self.unpack("list[Text]", out_field, span, cx)?;
        let appended = self.instantiate(&self.spec.lists.append.clone(), &[out, piece.0], span)?;
        let packed = self.instantiate(&self.spec.loops.pack.clone(), &[appended], span)?;
        cx.counter += 1;
        let name = format!("{}{}_repr{}", p, cx.fn_name, cx.counter);
        let value = self.spec.types.value.clone();
        cx.lifted.push(self.fn_node(&name, &[(acc, value.clone()), (item, value.clone())], &value, self.finish(vec![], packed, span), span));
        let init = self.instantiate(&self.spec.loops.pack.clone(), &[self.instantiate(&self.spec.lists.new.clone(), &[], span)?], span)?;
        let folded = self.instantiate(&self.spec.loops.fold.clone(), &[list, init, self.name_node(&name, span)], span)?;
        let tf = self.instantiate(&self.spec.loops.field.clone(), &[folded, int_lit(self.kind("int_lit"), 0, span)], span)?;
        let texts = self.unpack("list[Text]", tf, span, cx)?;
        let joined = (self.call_node("str_join", vec![texts, self.text_lit(", ", span)], span), text.clone());
        let opened = cat(self, lit(open), joined, cx)?;
        cat(self, opened, lit(close), cx)
    }

    fn sort_tuples(&self, xs: Typed, parts: &[String], rev: SchemaAstNode, span: &Span, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let p = self.spec.reserved_prefix.clone();
        let item = format!("{}item", p);
        let acc = format!("{}acc", p);
        // key(item) = key_0(item[0]) + key_1(item[1]) + ...
        let mut key: Option<Typed> = None;
        for (i, t) in parts.iter().enumerate() {
            let enc = match self.spec.sort_keys.get(t) {
                Some(e) => e.clone(),
                None => return err(format!("sorting tuples that hold {} is not supported", self.spell(t)), span),
            };
            let field = self.instantiate(&self.spec.loops.field.clone(), &[self.name_node(&item, span), int_lit(self.kind("int_lit"), i as i64, span)], span)?;
            let v = self.unpack(t, field, span, cx)?;
            let k = (self.instantiate(&enc, &[v], span)?, Some(self.spec.types.text.clone()));
            key = Some(match key {
                None => k,
                Some(prev) => self.dispatch_op("+", vec![prev, k], span, cx)?,
            });
        }
        let key = key.ok_or_else(|| ExpandError { message: "empty tuple".into(), span: span.clone() })?.0;
        // A lifted fold step collecting the keys and the items, so the list
        // itself is read once.
        let field = |me: &Self, k: i64| me.instantiate(&me.spec.loops.field.clone(), &[me.name_node(&acc, span), int_lit(me.kind("int_lit"), k, span)], span);
        let keys_so_far = self.unpack("list[Text]", field(self, 0)?, span, cx)?;
        let items_so_far = self.unpack("list[Value]", field(self, 1)?, span, cx)?;
        let keys2 = self.instantiate(&self.spec.lists.append.clone(), &[keys_so_far, key], span)?;
        let items2 = self.instantiate(&self.spec.lists.append.clone(), &[items_so_far, self.name_node(&item, span)], span)?;
        let packed = self.instantiate(&self.spec.loops.pack.clone(), &[keys2, items2], span)?;
        cx.counter += 1;
        let name = format!("{}{}_sortkey{}", p, cx.fn_name, cx.counter);
        let value = self.spec.types.value.clone();
        cx.lifted.push(self.fn_node(&name, &[(acc, value.clone()), (item, value.clone())], &value, self.finish(vec![], packed, span), span));
        let empty = self.instantiate(&self.spec.lists.new.clone(), &[], span)?;
        let init = self.instantiate(&self.spec.loops.pack.clone(), &[empty.clone(), empty], span)?;
        let folded = self.instantiate(&self.spec.loops.fold.clone(), &[xs.0, init, self.name_node(&name, span)], span)?;
        Ok((self.instantiate(&self.spec.sort_by_keys.clone(), &[folded, rev], span)?, xs.1))
    }

    fn dispatch_builtin(&self, name: &str, mut args: Vec<Typed>, span: &Span, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        // str(container) / print(container): Python's repr of it.
        if (name == self.spec.str_builtin || self.spec.joined_builtins.contains_key(name)) && args.len() == 1 {
            let t = args[0].1.clone().map(|t| self.resolve(&t, cx)).unwrap_or_default();
            if Self::is_list(&t) || Self::is_tuple(&t) || Self::is_dict(&t) {
                let text = self.repr_value(args.remove(0), span, cx)?;
                if name == self.spec.str_builtin {
                    return Ok(text);
                }
                args.push(text);
            }
        }
        let overloads = &self.spec.builtins[name];
        self.choose(&format!("`{}()`", name), overloads, args, span, cx)
    }

    fn dispatch_op(&self, op: &str, args: Vec<Typed>, span: &Span, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        let overloads = match self.spec.operators.get(op) {
            Some(o) => o,
            None => return err(format!("`{}` is not an operator in this language", op), span),
        };
        self.choose(&format!("`{}`", if op == "neg" { "-" } else { op }), overloads, args, span, cx)
    }

    fn choose(&self, what: &str, overloads: &[Overload], args: Vec<Typed>, span: &Span, cx: &mut FnCx) -> Result<Typed, ExpandError> {
        for (n, ty) in &args {
            if ty.is_none() {
                return self.soft(cx, format!(
                    "{} needs to know the type of this operand, and it is not written anywhere; annotate the variable it comes from (e.g. `x: int = ...`)",
                    what
                ), &n.span);
            }
        }
        let tys: Vec<String> = args.iter().map(|a| self.resolve(a.1.as_ref().unwrap(), cx)).collect();
        let mut found = overloads.iter().find(|o| o.args.len() == tys.len() && o.args.iter().zip(&tys).all(|(want, got)| self.matches(want, got)));
        // An argument whose element type is still pending (`keys = []`): the
        // first overload it can unify with fixes it (the first-store rule).
        if found.is_none() && tys.iter().any(|t| t.contains("[?")) {
            for o in overloads.iter().filter(|o| o.args.len() == tys.len()) {
                let saved = cx.pending.clone();
                let ok = o.args.iter().zip(&tys).all(|(want, got)| self.matches(want, got) || (!want.contains('*') && self.same_type(want, got, cx)));
                if ok {
                    found = Some(o);
                    break;
                }
                cx.pending = saved;
            }
        }
        let tys: Vec<String> = tys.iter().map(|t| self.resolve(t, cx)).collect();
        match found {
            Some(o) => {
                let ret = match o.ret.as_str() {
                    "$0" => tys[0].clone(),
                    "elem$0" => match Self::elem_of(&tys[0]) {
                        Some(e) if !e.starts_with('?') => e.to_string(),
                        _ => return err(format!("{} needs to know what this list holds; annotate it", what), span),
                    },
                    "list$0" => format!("list[{}]", tys[0]),
                    r => r.to_string(),
                };
                // Two lists of different element types do not mix.
                if o.args.len() == 2 && o.args.iter().all(|a| a == "list[*]") && !self.same_type(&tys[0], &tys[1], cx) {
                    return err(format!("{} is not defined for ({}); the lists must hold the same type", what, self.spell_all(&tys)), span);
                }
                let nodes: Vec<SchemaAstNode> = args.into_iter().map(|a| a.0).collect();
                let raw = self.instantiate(&o.emit, &nodes, span)?;
                Ok((raw, Some(ret)))
            }
            None => {
                let have: Vec<String> = overloads.iter().map(|o| format!("({})", o.args.iter().map(|a| if a == "list[*]" { format!("{}[...]", self.spec.types.list) } else { self.spell(a) }).collect::<Vec<_>>().join(", "))).collect();
                err(format!("{} is not defined for ({}); it takes {}", what, self.spell_all(&tys), have.join(" or ")), span)
            }
        }
    }

    fn call_node(&self, name: &str, args: Vec<SchemaAstNode>, span: &Span) -> SchemaAstNode {
        let arglist = mk(self.kind("arg_list"), span, vec![("args", SchemaValue::Nodes(args))]);
        mk(self.kind("call_out"), span, vec![
            ("name", SchemaValue::Token(ident(name, span))),
            ("args", SchemaValue::Node(Box::new(arglist))),
        ])
    }

    /// Instantiate a call template. An argument used more than once must be a
    /// name or a literal: duplicating anything else would evaluate it twice.
    fn instantiate(&self, template: &str, args: &[SchemaAstNode], span: &Span) -> Result<SchemaAstNode, ExpandError> {
        let t = parse_template(template).map_err(|m| ExpandError { message: m, span: span.clone() })?;
        let mut uses = vec![0usize; args.len()];
        count_uses(&t, &mut uses);
        for (k, n) in uses.iter().enumerate() {
            if *n > 1 && !self.simple(&args[k]) {
                return err("this expression would be evaluated twice here; assign it to a name first", &args[k].span);
            }
        }
        self.build(&t, args, span)
    }

    fn build(&self, t: &Tmpl, args: &[SchemaAstNode], span: &Span) -> Result<SchemaAstNode, ExpandError> {
        match t {
            Tmpl::Arg(k) => args.get(*k).cloned().ok_or_else(|| ExpandError {
                message: format!("template refers to ${} but only {} argument(s) exist", k, args.len()),
                span: span.clone(),
            }),
            Tmpl::All => err("`$*` is only allowed as the whole argument list", span),
            Tmpl::Unit => Ok(mk(self.kind("none_lit"), span, vec![])),
            Tmpl::Int(v) => Ok(int_lit(self.kind("int_lit"), *v, span)),
            Tmpl::Text(s) => {
                let tok = Token::new(TokenKind::StringLit, format!("\"{}\"", s), span.clone());
                Ok(mk(self.kind("text_lit"), span, vec![("value", SchemaValue::Token(tok))]))
            }
            Tmpl::Bool(v) => Ok(self.bool_lit(*v, span)),
            Tmpl::Call(name, inner) => {
                let mut built = Vec::new();
                for i in inner {
                    if matches!(i, Tmpl::All) {
                        built.extend(args.iter().cloned());
                    } else {
                        built.push(self.build(i, args, span)?);
                    }
                }
                Ok(self.call_node(name, built, span))
            }
        }
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

struct Sig {
    ret: String,
    params: Vec<String>,
    names: Vec<String>,
    /// Default value expressions (as written), by parameter.
    defaults: Vec<Option<SchemaAstNode>>,
}

struct FnCx<'a> {
    sigs: &'a HashMap<String, Sig>,
    /// The author fn's declared return type (also what a `return` inside a loop returns).
    ret: String,
    fn_name: String,
    counter: u32,
    fn_span: Span,
    /// Lifted loop bodies, emitted after the fn as siblings.
    lifted: Vec<SchemaAstNode>,
    /// First-store rule: element types of `[]` / `{}` literals, fixed by their
    /// first store. Keyed by the literal's source position, so the discovery
    /// pass and the real pass agree on which literal is which.
    pending: BTreeMap<String, Option<String>>,
    /// Discovery pass: a type not known yet is a placeholder, not an error,
    /// so the pass reaches the stores that fix it.
    discover: bool,
    /// Variables bound to an infinity sentinel and later to ints: pyaxc's
    /// rule, `float("inf")` as a starting value is the largest int.
    int_sentinels: BTreeSet<String>,
    consts: &'a HashMap<String, (SchemaAstNode, Option<String>)>,
    /// Constants being inlined, to refuse one defined through itself.
    const_stack: Vec<String>,
    /// Names introduced by `hoist_stmt`: reserved-prefixed, but assigned
    /// through the ordinary statement path.
    hoisted: BTreeSet<String>,
    /// Names that exist only inside a loop body, as a loop variable, or in
    /// some branches of an `if`: read after it, they get an error that says
    /// so rather than falling through as a registry name.
    scoped: BTreeMap<String, Scoped>,
    /// Registry signatures, when the pipeline has a scope.
    reg: Option<&'a RegistrySigs>,
}

#[derive(Clone, Copy)]
enum Scoped {
    LoopVar,
    LoopBody,
    Branch,
}

impl FnCx<'_> {
    /// The pending slot for the literal at `span`.
    fn pend(&mut self, tag: &str, span: &Span) -> String {
        let id = format!("{}{}", tag, span.start);
        self.pending.entry(id.clone()).or_insert(None);
        id
    }

    /// A fresh output name under the reserved prefix; deterministic per fn.
    fn fresh(&mut self, prefix: &str, base: &str) -> String {
        self.counter += 1;
        format!("{}{}_{}", prefix, base, self.counter)
    }
}

#[derive(Clone)]
struct Var {
    ty: Option<String>,
    /// The output name currently holding this variable's value.
    cur: String,
    param: bool,
    /// Another name this list may share its value with (Python aliasing).
    shared: Option<String>,
    /// Bound to `float("inf")` / `-float("inf")` / `math.inf` (+1 / -1).
    sentinel: Option<i8>,
}

#[derive(Clone, Default)]
struct Env {
    vars: BTreeMap<String, Var>,
    /// Lists a loop is iterating; updating one is refused.
    iterating: BTreeSet<String>,
    /// Slots a loop needs bound before it (done flag, return value).
    pending_bindings: Vec<(String, SchemaAstNode, String)>,
}

#[derive(Clone)]
struct Frame {
    /// (name, type) in pack order.
    state: Vec<(String, String)>,
}

enum Mode {
    Fn,
    Loop(Rc<Frame>),
}

enum IterKind {
    EnumerateChars(Token),
    List(String),
    Tuple(Vec<String>),
    Chars(Token),
    Enumerate((Token, String), i64),
    Zip((Token, String), (Token, String)),
}

struct IterPlan {
    list: SchemaAstNode,
    kind: IterKind,
    targets: Vec<Token>,
    /// The iterable as written, when the body needs to read it.
    source: Option<SchemaAstNode>,
    /// The name being iterated, when it is a list name.
    iterating: Option<String>,
}

impl IterPlan {
    /// Bind the loop variable(s) from one erased item, as output bindings in
    /// `pre`, into the lifted fn's `env`.
    fn bind(&self, item: &str, eng: &ExpandEngine, env: &mut Env, cx: &mut FnCx, pre: &mut Vec<SchemaAstNode>, span: &Span) -> Result<(), ExpandError> {
        let p = eng.spec.reserved_prefix.clone();
        let it = eng.name_node(item, span);
        let int = eng.spec.types.int.clone();
        let bind_one = |name: &str, (node, ty): Typed, env: &mut Env, cx: &mut FnCx, pre: &mut Vec<SchemaAstNode>| -> String {
            let cur = cx.fresh(&p, name.trim_start_matches(p.as_str()));
            pre.push(eng.binding(&cur, node, span));
            env.vars.insert(name.to_string(), Var { ty, cur: cur.clone(), param: false, shared: None, sentinel: None });
            cur
        };
        let elem_at = |n: &(Token, String), i: SchemaAstNode, env: &Env, cx: &FnCx| -> Result<Typed, ExpandError> {
            let (xs, _) = eng.read_var(&n.0, env, cx)?;
            let e = ExpandEngine::elem_of(&n.1).unwrap_or("").to_string();
            let raw = eng.instantiate(&eng.spec.lists.get.clone(), &[xs, i], span)?;
            Ok((eng.unpack(&e, raw, span, cx)?, Some(e)))
        };
        let t0 = self.targets[0].lexeme.clone();
        match &self.kind {
            IterKind::List(e) => {
                let v = eng.unpack(e, it, span, cx)?;
                bind_one(&t0, (v, Some(e.clone())), env, cx, pre);
            }
            IterKind::EnumerateChars(s) => {
                let i = eng.unpack(&int, it, span, cx)?;
                let icur = bind_one(&t0, (i, Some(int.clone())), env, cx, pre);
                let (sv, _) = eng.read_var(s, env, cx)?;
                let c = eng.instantiate(&eng.spec.text.get.clone(), &[sv, eng.name_node(&icur, span)], span)?;
                bind_one(&self.targets[1].lexeme.clone(), (c, Some(eng.spec.types.text.clone())), env, cx, pre);
            }
            IterKind::Tuple(parts) => {
                for (k, ty) in parts.iter().enumerate() {
                    let field = eng.instantiate(&eng.spec.loops.field.clone(), &[it.clone(), int_lit(eng.kind("int_lit"), k as i64, span)], span)?;
                    let v = eng.unpack(ty, field, span, cx)?;
                    bind_one(&self.targets[k].lexeme.clone(), (v, Some(ty.clone())), env, cx, pre);
                }
            }
            IterKind::Chars(s) => {
                let (sv, _) = eng.read_var(s, env, cx)?;
                let i = eng.unpack(&int, it, span, cx)?;
                let c = eng.instantiate(&eng.spec.text.get.clone(), &[sv, i], span)?;
                bind_one(&t0, (c, Some(eng.spec.types.text.clone())), env, cx, pre);
            }
            IterKind::Enumerate(xs, start) => {
                let i = eng.unpack(&int, it, span, cx)?;
                let pos = bind_one(&format!("{}pos", p), (i, Some(int.clone())), env, cx, pre);
                let shown = if *start == 0 {
                    eng.name_node(&pos, span)
                } else {
                    eng.call_node("int_add", vec![eng.name_node(&pos, span), int_lit(eng.kind("int_lit"), *start, span)], span)
                };
                bind_one(&t0, (shown, Some(int.clone())), env, cx, pre);
                let x = elem_at(xs, eng.name_node(&pos, span), env, cx)?;
                bind_one(&self.targets[1].lexeme.clone(), x, env, cx, pre);
            }
            IterKind::Zip(a, b) => {
                let i = eng.unpack(&int, it, span, cx)?;
                let icur = bind_one(&format!("{}i", p), (i, Some(int.clone())), env, cx, pre);
                let x = elem_at(a, eng.name_node(&icur, span), env, cx)?;
                bind_one(&t0, x, env, cx, pre);
                let y = elem_at(b, eng.name_node(&icur, span), env, cx)?;
                bind_one(&self.targets[1].lexeme.clone(), y, env, cx, pre);
            }
        }
        Ok(())
    }
}

enum Tmpl {
    Arg(usize),
    /// `$*`: every argument, in order (only as a whole argument list item).
    All,
    /// `@unit`: the surface's unit literal (for callables that take `Unit`).
    Unit,
    Int(i64),
    Text(String),
    Bool(bool),
    Call(String, Vec<Tmpl>),
}

fn count_uses(t: &Tmpl, uses: &mut [usize]) {
    match t {
        Tmpl::Arg(k) => {
            if let Some(u) = uses.get_mut(*k) {
                *u += 1;
            }
        }
        Tmpl::All => uses.iter_mut().for_each(|u| *u += 1),
        Tmpl::Call(_, inner) => inner.iter().for_each(|i| count_uses(i, uses)),
        _ => {}
    }
}

fn parse_template(s: &str) -> Result<Tmpl, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut pos = 0;
    let t = parse_tmpl(&chars, &mut pos)?;
    skip_ws(&chars, &mut pos);
    if pos != chars.len() {
        return Err(format!("template `{}`: trailing text", s));
    }
    Ok(t)
}

fn skip_ws(c: &[char], pos: &mut usize) {
    while c.get(*pos).map_or(false, |ch| ch.is_whitespace()) {
        *pos += 1;
    }
}

fn parse_tmpl(c: &[char], pos: &mut usize) -> Result<Tmpl, String> {
    skip_ws(c, pos);
    match c.get(*pos) {
        Some('$') => {
            *pos += 1;
            if c.get(*pos) == Some(&'*') {
                *pos += 1;
                return Ok(Tmpl::All);
            }
            let start = *pos;
            while c.get(*pos).map_or(false, |ch| ch.is_ascii_digit()) {
                *pos += 1;
            }
            let n: String = c[start..*pos].iter().collect();
            return n.parse().map(Tmpl::Arg).map_err(|_| "`$` must be followed by an argument number or `*`".to_string());
        }
        Some('@') => {
            if c[*pos..].starts_with(&['@', 'u', 'n', 'i', 't']) {
                *pos += 5;
                return Ok(Tmpl::Unit);
            }
            return Err("unknown `@` form (only `@unit`)".to_string());
        }
        Some('"') => {
            *pos += 1;
            let start = *pos;
            while c.get(*pos).map_or(false, |ch| *ch != '"') {
                *pos += 1;
            }
            if c.get(*pos) != Some(&'"') {
                return Err("unterminated string in template".to_string());
            }
            let s: String = c[start..*pos].iter().collect();
            *pos += 1;
            return Ok(Tmpl::Text(s));
        }
        Some(ch) if ch.is_ascii_digit() || *ch == '-' => {
            let start = *pos;
            *pos += 1;
            while c.get(*pos).map_or(false, |ch| ch.is_ascii_digit()) {
                *pos += 1;
            }
            let n: String = c[start..*pos].iter().collect();
            return n.parse().map(Tmpl::Int).map_err(|_| format!("bad integer `{}` in template", n));
        }
        _ => {}
    }
    let start = *pos;
    while c.get(*pos).map_or(false, |ch| ch.is_ascii_alphanumeric() || *ch == '_') {
        *pos += 1;
    }
    let name: String = c[start..*pos].iter().collect();
    skip_ws(c, pos);
    if name == "true" || name == "false" {
        return Ok(Tmpl::Bool(name == "true"));
    }
    if name.is_empty() || c.get(*pos) != Some(&'(') {
        return Err("a template is `name(args)`, `$n`, `$*`, `@unit`, or a literal".to_string());
    }
    *pos += 1;
    let mut args = Vec::new();
    skip_ws(c, pos);
    if c.get(*pos) == Some(&')') {
        *pos += 1;
        return Ok(Tmpl::Call(name, args));
    }
    loop {
        args.push(parse_tmpl(c, pos)?);
        skip_ws(c, pos);
        match c.get(*pos) {
            Some(',') => *pos += 1,
            Some(')') => {
                *pos += 1;
                return Ok(Tmpl::Call(name, args));
            }
            _ => return Err("expected `,` or `)` in template".to_string()),
        }
    }
}

fn ident(name: &str, span: &Span) -> Token {
    Token::new(TokenKind::Ident, name.to_string(), span.clone())
}

fn int_lit(kind: &str, v: i64, span: &Span) -> SchemaAstNode {
    let tok = Token::new(TokenKind::IntLit, v.to_string(), span.clone());
    mk(kind, span, vec![("value", SchemaValue::Token(tok))])
}

fn int_value(n: &SchemaAstNode, int_kind: &str) -> Option<i64> {
    if n.kind != int_kind {
        return None;
    }
    match n.fields.get("value") {
        Some(SchemaValue::Token(t)) => t.lexeme.parse().ok(),
        _ => None,
    }
}

fn mk(kind: &str, span: &Span, fields: Vec<(&str, SchemaValue)>) -> SchemaAstNode {
    SchemaAstNode {
        kind: kind.to_string(),
        fields: fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        annotations: Default::default(),
        span: span.clone(),
    }
}

fn node<'a>(n: &'a SchemaAstNode, field: &str) -> Result<&'a SchemaAstNode, ExpandError> {
    match n.fields.get(field) {
        Some(SchemaValue::Node(x)) => Ok(x),
        _ => err(format!("{} has no `{}` node (expand contract)", n.kind, field), &n.span),
    }
}

fn nodes<'a>(n: &'a SchemaAstNode, field: &str) -> Vec<&'a SchemaAstNode> {
    match n.fields.get(field) {
        Some(SchemaValue::Nodes(xs)) => xs.iter().collect(),
        Some(SchemaValue::Node(x)) => vec![x.as_ref()],
        _ => vec![],
    }
}

fn token<'a>(n: &'a SchemaAstNode, field: &str) -> Result<&'a Token, ExpandError> {
    match n.fields.get(field) {
        Some(SchemaValue::Token(t)) => Ok(t),
        _ => err(format!("{} has no `{}` token (expand contract)", n.kind, field), &n.span),
    }
}

fn tokens(n: &SchemaAstNode, field: &str) -> Vec<Token> {
    match n.fields.get(field) {
        Some(SchemaValue::Tokens(ts)) => ts.clone(),
        Some(SchemaValue::Token(t)) => vec![t.clone()],
        _ => vec![],
    }
}

fn span_end(stmts: &[SchemaAstNode], fallback: &Span) -> Span {
    stmts.last().map(|s| Span::new(s.span.end, s.span.end)).unwrap_or_else(|| fallback.clone())
}
