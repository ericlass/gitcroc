//! The rules every view's accessibility tree must meet (M6-25a), over the
//! JSON of `Window::debug_a11y_tree_json`. Each problem names the node and
//! the rule. A known gap is allowed only for one element and one rule,
//! with its reason, never for a whole role. The unit tests at the end need
//! no display; `tests/a11y_checker.rs` runs them.

use std::collections::HashMap;
use std::fmt;

use serde_json::Value;

/// Roles a user operates. A node with one of these, or one that takes a
/// click, the focus or a value, is interactive.
const INTERACTIVE_ROLES: &[&str] = &[
    "Button",
    "CheckBox",
    "ComboBox",
    "TextInput",
    "MultilineTextInput",
    "SearchInput",
    "ListBox",
    "ListBoxOption",
    "Tree",
    "TreeItem",
    "Tab",
    "MenuItem",
    "MenuItemCheckBox",
    "MenuItemRadio",
    "RadioButton",
    "Switch",
    "Slider",
    "Link",
    "Dialog",
    "AlertDialog",
];
const INTERACTIVE_ACTIONS: &[&str] = &["Click", "Focus", "SetValue"];

/// A rule of M6-25a's bar (README decisions table, "Accessibility bar in
/// M6", musts 1, 2 and 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// An interactive node has a role.
    Role,
    /// An interactive node has a name.
    Name,
    /// A list row is a `ListBoxOption` inside a `ListBox`, says whether it
    /// is selected, and carries its position, as "43 of 1,000".
    ListRow,
    /// A focused list reports its selected row as the focus.
    ActiveRow,
    /// A tree row is a `TreeItem` inside a `Tree`, with selected and level
    /// set, and a description that says them where Linux cannot.
    TreeRow,
    /// An icon that carries meaning has a name.
    Icon,
    /// A dialog's text, from the first paragraph directly in it, is its
    /// description, which a screen reader reads as the dialog opens.
    Dialog,
}

impl Rule {
    fn says(self) -> &'static str {
        match self {
            Self::Role => "interactive but has no role",
            Self::Name => "has no name",
            Self::ListRow => "list row without its selection or position",
            Self::ActiveRow => "focused list does not report its selected row",
            Self::TreeRow => "tree row without selected, level, or its description",
            Self::Icon => "icon without a name",
            Self::Dialog => "dialog without its text as its description",
        }
    }
}

/// A node that breaks a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The element's id, or the node's key when it has none.
    pub element: String,
    /// Where the element was built, in debug builds.
    pub source: Option<String>,
    pub rule: Rule,
    /// What the node has instead.
    pub detail: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.element, self.rule.says())?;
        if !self.detail.is_empty() {
            write!(f, " ({})", self.detail)?;
        }
        if let Some(source) = &self.source {
            write!(f, " at {source}")?;
        }
        Ok(())
    }
}

/// One element allowed to break one rule, and why.
pub struct Gap {
    pub element: &'static str,
    pub rule: Rule,
    pub reason: &'static str,
}

/// One dump, indexed.
pub struct Tree<'a> {
    nodes: HashMap<&'a str, &'a Value>,
    parents: HashMap<&'a str, &'a str>,
    focus: Option<&'a str>,
    active: Option<&'a str>,
}

impl<'a> Tree<'a> {
    pub fn new(dump: &'a Value) -> Self {
        let mut nodes = HashMap::new();
        let mut parents = HashMap::new();
        if let Some(all) = dump["nodes"].as_object() {
            for (key, node) in all {
                nodes.insert(key.as_str(), node);
                for child in node["children"].as_array().into_iter().flatten() {
                    if let Some(child) = child.as_str() {
                        parents.insert(child, key.as_str());
                    }
                }
            }
        }
        Self {
            nodes,
            parents,
            focus: dump["gpui_focus"].as_str(),
            active: dump["active_descendant_focus"].as_str(),
        }
    }

    /// Node keys, in a stable order.
    fn keys(&self) -> Vec<&'a str> {
        let mut keys: Vec<_> = self.nodes.keys().copied().collect();
        keys.sort_by_key(|key| (key.len(), *key));
        keys
    }

    fn aria(&self, key: &str) -> &'a Value {
        &self.nodes[key]["aria"]
    }

    fn role(&self, key: &str) -> &'a str {
        self.aria(key)["role"].as_str().unwrap_or("Unknown")
    }

    fn text(&self, key: &str, field: &str) -> &'a str {
        self.aria(key)[field].as_str().map_or("", str::trim)
    }

    fn actions(&self, key: &str) -> Vec<&'a str> {
        self.aria(key)["on_action"]
            .as_array()
            .map(|actions| actions.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    /// The element id as GPUI prints it, `Name("x")` as `x` and
    /// `NamedInteger("x", 3)` as `x[3]`.
    pub fn element(&self, key: &str) -> String {
        let Some(id) = self.nodes[key]["element_id"].as_str() else {
            return key.to_owned();
        };
        if let Some(name) = id
            .strip_prefix("Name(\"")
            .and_then(|rest| rest.strip_suffix("\")"))
        {
            return name.to_owned();
        }
        id.strip_prefix("NamedInteger(\"")
            .and_then(|rest| rest.strip_suffix(')'))
            .and_then(|rest| rest.split_once("\", "))
            .map_or_else(|| id.to_owned(), |(name, at)| format!("{name}[{at}]"))
    }

    fn children(&self, key: &str) -> Vec<&'a str> {
        self.nodes[key]["children"]
            .as_array()
            .map(|children| children.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    fn ancestors(&self, key: &'a str) -> impl Iterator<Item = &'a str> + '_ {
        std::iter::successors(self.parents.get(key).copied(), |at| {
            self.parents.get(at).copied()
        })
    }

    fn inside(&self, key: &'a str, role: &str) -> Option<&'a str> {
        self.ancestors(key).find(|at| self.role(at) == role)
    }

    fn problem(&self, key: &str, rule: Rule, detail: String) -> Problem {
        Problem {
            element: self.element(key),
            source: self.nodes[key]["source_location"]
                .as_str()
                .map(str::to_owned),
            rule,
            detail,
        }
    }

    /// The key of the node with element id `element`.
    pub fn find(&self, element: &str) -> Option<&'a str> {
        self.keys()
            .into_iter()
            .find(|key| self.element(key) == element)
    }

    /// Whether node `key` takes the focus.
    pub fn focusable(&self, key: &str) -> bool {
        self.actions(key).contains(&"Focus")
    }

    pub fn label(&self, key: &str) -> &'a str {
        self.text(key, "label")
    }

    pub fn role_of(&self, key: &str) -> &'a str {
        self.role(key)
    }
}

/// Whether `text` carries a position such as "43 of 1,000".
fn has_position(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    let number = |word: &str| {
        let word = word.trim_end_matches([',', '.', ';', ')']);
        !word.is_empty() && word.chars().all(|c| c.is_ascii_digit() || c == ',')
    };
    words
        .windows(3)
        .any(|w| number(w[0]) && w[1] == "of" && number(w[2]))
}

/// Every problem of `dump`, less the allowed `gaps`.
pub fn check(dump: &Value, gaps: &[Gap]) -> Vec<Problem> {
    let tree = Tree::new(dump);
    let mut problems = Vec::new();
    for key in tree.keys() {
        problems.extend(names_and_roles(&tree, key));
        problems.extend(list_row(&tree, key));
        problems.extend(tree_row(&tree, key));
        problems.extend(icon(&tree, key));
        problems.extend(dialog(&tree, key));
    }
    problems.extend(active_row(&tree));
    problems.retain(|problem| {
        !gaps
            .iter()
            .any(|gap| gap.element == problem.element && gap.rule == problem.rule)
    });
    problems
}

fn names_and_roles(tree: &Tree, key: &str) -> Option<Problem> {
    let role = tree.role(key);
    let actions = tree.actions(key);
    let interactive = INTERACTIVE_ROLES.contains(&role)
        || actions
            .iter()
            .any(|action| INTERACTIVE_ACTIONS.contains(action));
    if !interactive {
        return None;
    }
    if role == "Unknown" || role == "GenericContainer" {
        return Some(tree.problem(key, Rule::Role, actions.join(", ")));
    }
    tree.label(key)
        .is_empty()
        .then(|| tree.problem(key, Rule::Name, role.to_owned()))
}

fn list_row(tree: &Tree, key: &str) -> Option<Problem> {
    if tree.role(key) != "ListBoxOption" {
        return None;
    }
    let mut missing = Vec::new();
    if tree.inside(key, "ListBox").is_none() {
        missing.push("not in a ListBox");
    }
    if tree.aria(key)["selected"].as_bool().is_none() {
        missing.push("no selected state");
    }
    let said = format!("{} {}", tree.label(key), tree.text(key, "description"));
    if !has_position(&said) {
        missing.push("no position in its name or description");
    }
    (!missing.is_empty()).then(|| tree.problem(key, Rule::ListRow, missing.join(", ")))
}

/// A focused `ListBox` or `Tree` with a selected row reports a row as the
/// focus, so a screen reader reads the row, not the list.
fn active_row(tree: &Tree) -> Option<Problem> {
    let focus = tree.focus?;
    let list = std::iter::once(focus)
        .chain(tree.ancestors(focus))
        .find(|at| matches!(tree.role(at), "ListBox" | "Tree"))?;
    let selected = tree.keys().into_iter().any(|key| {
        matches!(tree.role(key), "ListBoxOption" | "TreeItem")
            && tree.aria(key)["selected"].as_bool() == Some(true)
            && tree.ancestors(key).any(|at| at == list)
    });
    let reported = tree
        .active
        .is_some_and(|active| tree.ancestors(active).any(|at| at == list));
    (selected && !reported).then(|| {
        tree.problem(
            list,
            Rule::ActiveRow,
            "a row is selected but none is the active descendant".to_owned(),
        )
    })
}

fn tree_row(tree: &Tree, key: &str) -> Option<Problem> {
    if tree.role(key) != "TreeItem" {
        return None;
    }
    let aria = tree.aria(key);
    let description = tree.text(key, "description");
    let mut missing = Vec::new();
    if tree.inside(key, "Tree").is_none() {
        missing.push("not in a Tree".to_owned());
    }
    if aria["selected"].as_bool().is_none() {
        missing.push("no selected state".to_owned());
    }
    match aria["level"].as_u64() {
        None => missing.push("no level".to_owned()),
        // AccessKit's AT-SPI adapter maps neither level nor expanded
        // (M6-04 finding 7), so the description says them.
        Some(level) if !description.contains(&format!("level {level}")) => {
            missing.push(format!("description lacks \"level {level}\""));
        }
        Some(_) => {}
    }
    let state = match aria["expanded"].as_bool() {
        Some(true) => Some("expanded"),
        Some(false) => Some("collapsed"),
        None => None,
    };
    if let Some(state) = state
        && !description.split([',', ' ']).any(|word| word == state)
    {
        missing.push(format!("description lacks \"{state}\""));
    }
    (!missing.is_empty()).then(|| tree.problem(key, Rule::TreeRow, missing.join(", ")))
}

fn icon(tree: &Tree, key: &str) -> Option<Problem> {
    (tree.role(key) == "Image" && tree.label(key).is_empty())
        .then(|| tree.problem(key, Rule::Icon, String::new()))
}

fn dialog(tree: &Tree, key: &str) -> Option<Problem> {
    if !matches!(tree.role(key), "Dialog" | "AlertDialog") {
        return None;
    }
    // An unnamed dialog breaks the name rule already.
    let mut missing = Vec::new();
    let description = tree.text(key, "description");
    let first = tree
        .children(key)
        .into_iter()
        .find(|child| tree.role(child) == "Paragraph");
    if let Some(first) = first
        && !description.contains(tree.label(first))
    {
        missing.push(format!("description lacks \"{}\"", tree.label(first)));
    }
    (!missing.is_empty()).then(|| tree.problem(key, Rule::Dialog, missing.join(", ")))
}

/// Elements `expected` have their role, as `("history-list", "ListBox")`.
pub fn roles(dump: &Value, expected: &[(&str, &str)]) -> Vec<String> {
    let tree = Tree::new(dump);
    expected
        .iter()
        .filter_map(|(id, role)| match tree.find(id) {
            None => Some(format!("{id}: not in the tree")),
            Some(key) if tree.role_of(key) != *role => {
                Some(format!("{id}: a {}, not a {role}", tree.role_of(key)))
            }
            Some(_) => None,
        })
        .collect()
}

/// UI-35: the window buttons `ids` are drawn, named, and outside the Tab
/// order, since the keyboard has Minimize, Zoom and Quit.
pub fn window_buttons(dump: &Value, ids: &[&str]) -> Vec<String> {
    let tree = Tree::new(dump);
    let mut problems = Vec::new();
    for id in ids {
        match tree.find(id) {
            None => problems.push(format!("{id}: not in the tree")),
            Some(key) => {
                if tree.role_of(key) != "Button" {
                    problems.push(format!("{id}: a {}, not a Button", tree.role_of(key)));
                }
                if tree.label(key).is_empty() {
                    problems.push(format!("{id}: has no name"));
                }
                if tree.focusable(key) {
                    problems.push(format!("{id}: is a tab stop"));
                }
            }
        }
    }
    problems
}

// The harness, which has no test harness, compiles these too.
#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A window holding `nodes`, which name their children; the nodes no
    /// other node holds are the window's.
    fn dump(nodes: Value) -> Value {
        let mut all = nodes.as_object().unwrap().clone();
        let held: Vec<Value> = all
            .values()
            .flat_map(|node| node["children"].as_array().cloned().unwrap_or_default())
            .collect();
        let children: Vec<Value> = all
            .keys()
            .map(|key| json!(key))
            .filter(|key| !held.contains(key))
            .collect();
        all.insert(
            "root".into(),
            json!({ "children": children, "aria": { "role": "Window", "label": "gitcroc" } }),
        );
        json!({ "root": "root", "nodes": all })
    }

    fn rules(dump: &Value) -> Vec<(String, Rule)> {
        check(dump, &[])
            .into_iter()
            .map(|problem| (problem.element, problem.rule))
            .collect()
    }

    #[test]
    fn a_button_without_a_name_is_named() {
        let tree = dump(json!({
            "b": { "element_id": "Name(\"fetch\")", "aria": { "role": "Button", "on_action": ["Click", "Focus"] } },
            "c": { "element_id": "Name(\"ok\")", "aria": { "role": "Button", "label": "OK", "on_action": ["Click"] } }
        }));
        assert_eq!(rules(&tree), vec![("fetch".to_owned(), Rule::Name)]);
    }

    #[test]
    fn a_clickable_node_without_a_role_is_named() {
        let tree = dump(json!({
            "b": { "element_id": "Name(\"row\")", "aria": { "role": "Unknown", "on_action": ["Click"] } }
        }));
        assert_eq!(rules(&tree), vec![("row".to_owned(), Rule::Role)]);
    }

    #[test]
    fn a_list_row_needs_a_list_its_selection_and_its_position() {
        let tree = dump(json!({
            "l": { "children": ["r", "s"], "element_id": "Name(\"history-list\")", "aria": { "role": "ListBox", "label": "History" } },
            "r": { "element_id": "Name(\"history-row-0\")", "aria": { "role": "ListBoxOption", "label": "Fix the build", "selected": false } },
            "s": { "element_id": "Name(\"history-row-1\")", "aria": { "role": "ListBoxOption", "label": "Add tests", "description": "2 of 1,000", "selected": false } },
            "t": { "element_id": "Name(\"stray\")", "aria": { "role": "ListBoxOption", "label": "Stray, 1 of 1" } }
        }));
        let problems = check(&tree, &[]);
        assert_eq!(
            problems
                .iter()
                .map(|p| (p.element.as_str(), p.rule, p.detail.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (
                    "history-row-0",
                    Rule::ListRow,
                    "no position in its name or description"
                ),
                (
                    "stray",
                    Rule::ListRow,
                    "not in a ListBox, no selected state"
                ),
            ]
        );
    }

    #[test]
    fn a_focused_list_must_report_its_selected_row() {
        let rows = json!({
            "l": { "children": ["r"], "element_id": "Name(\"files\")", "aria": { "role": "ListBox", "label": "Files", "on_action": ["Focus"] } },
            "r": { "element_id": "Name(\"files-row-0\")", "aria": { "role": "ListBoxOption", "label": "a.txt, 1 of 1", "selected": true } }
        });
        let mut tree = dump(rows);
        tree["gpui_focus"] = json!("l");
        assert_eq!(rules(&tree), vec![("files".to_owned(), Rule::ActiveRow)]);
        tree["active_descendant_focus"] = json!("r");
        assert_eq!(rules(&tree), vec![]);
    }

    #[test]
    fn a_tree_row_says_its_level_and_state_in_its_description() {
        let tree = dump(json!({
            "t": { "children": ["a", "b"], "element_id": "Name(\"tree-list\")", "aria": { "role": "Tree", "label": "Repositories" } },
            "a": { "element_id": "Name(\"tree-row-ungrouped\")", "aria": { "role": "TreeItem", "label": "Ungrouped", "selected": false, "expanded": true, "level": 1, "description": "expanded, level 1" } },
            "b": { "element_id": "Name(\"tree-row-repo:r1\")", "aria": { "role": "TreeItem", "label": "api", "selected": false, "expanded": false, "level": 2 } }
        }));
        let problems = check(&tree, &[]);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].element, "tree-row-repo:r1");
        assert_eq!(problems[0].rule, Rule::TreeRow);
        assert_eq!(
            problems[0].detail,
            "description lacks \"level 2\", description lacks \"collapsed\""
        );
    }

    #[test]
    fn an_icon_without_a_name_is_named() {
        let tree = dump(json!({
            "i": { "element_id": "Name(\"tree-row-repo:r1-dirty\")", "aria": { "role": "Image" } },
            "j": { "element_id": "Name(\"tree-row-repo:r2-dirty\")", "aria": { "role": "Image", "label": "Uncommitted changes" } }
        }));
        assert_eq!(
            rules(&tree),
            vec![("tree-row-repo:r1-dirty".to_owned(), Rule::Icon)]
        );
    }

    #[test]
    fn a_dialog_needs_its_text_as_its_description() {
        let tree = dump(json!({
            "d": { "children": ["p", "f"], "element_id": "Name(\"discard-dialog\")", "aria": { "role": "AlertDialog", "label": "Discard changes?" } },
            "p": { "element_id": "Name(\"discard-text\")", "aria": { "role": "Paragraph", "label": "They cannot be restored." } },
            "f": { "children": ["q"], "element_id": "Name(\"discard-files\")", "aria": { "role": "List", "label": "Files" } },
            "q": { "element_id": "Name(\"deeper\")", "aria": { "role": "Paragraph", "label": "Not the dialog's own text." } },
            "e": { "element_id": "Name(\"settings-dialog\")", "aria": { "role": "Dialog" } },
            "g": { "children": ["h"], "element_id": "Name(\"name-dialog\")", "aria": { "role": "Dialog", "label": "New group", "description": "Groups gather repos. Name it." } },
            "h": { "element_id": "Name(\"name-text\")", "aria": { "role": "Paragraph", "label": "Groups gather repos." } }
        }));
        let problems: Vec<_> = check(&tree, &[])
            .into_iter()
            .map(|p| (p.element, p.rule, p.detail))
            .collect();
        assert_eq!(
            problems,
            vec![
                (
                    "discard-dialog".to_owned(),
                    Rule::Dialog,
                    "description lacks \"They cannot be restored.\"".to_owned()
                ),
                (
                    "settings-dialog".to_owned(),
                    Rule::Name,
                    "Dialog".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_gap_allows_one_element_and_one_rule_only() {
        let tree = dump(json!({
            "a": { "element_id": "Name(\"one\")", "aria": { "role": "Button" } },
            "b": { "element_id": "Name(\"two\")", "aria": { "role": "Button" } }
        }));
        let gaps = [Gap {
            element: "one",
            rule: Rule::Name,
            reason: "a test",
        }];
        let left: Vec<String> = check(&tree, &gaps).into_iter().map(|p| p.element).collect();
        assert_eq!(left, vec!["two".to_owned()]);
        let other = [Gap {
            element: "one",
            rule: Rule::Icon,
            reason: "a test",
        }];
        assert_eq!(check(&tree, &other).len(), 2);
    }

    #[test]
    fn window_buttons_must_be_named_buttons_outside_the_tab_order() {
        let tree = dump(json!({
            "a": { "element_id": "Name(\"title-minimize\")", "aria": { "role": "Button", "label": "Minimize", "on_action": ["Click"] } },
            "b": { "element_id": "Name(\"title-close\")", "aria": { "role": "Button", "on_action": ["Click", "Focus"] } }
        }));
        assert_eq!(
            window_buttons(&tree, &["title-minimize", "title-close", "title-zoom"]),
            vec![
                "title-close: has no name".to_owned(),
                "title-close: is a tab stop".to_owned(),
                "title-zoom: not in the tree".to_owned(),
            ]
        );
    }

    #[test]
    fn lists_must_have_the_role_they_are_expected_to() {
        let tree = dump(json!({
            "a": { "element_id": "Name(\"history-list\")", "aria": { "role": "List", "label": "History" } },
            "b": { "element_id": "Name(\"tree-list\")", "aria": { "role": "Tree", "label": "Repositories" } }
        }));
        assert_eq!(
            roles(
                &tree,
                &[
                    ("history-list", "ListBox"),
                    ("tree-list", "Tree"),
                    ("job-rows", "ListBox")
                ]
            ),
            vec![
                "history-list: a List, not a ListBox".to_owned(),
                "job-rows: not in the tree".to_owned(),
            ]
        );
    }

    #[test]
    fn positions_are_found_in_words() {
        assert!(has_position("Fix the build, 43 of 1,000"));
        assert!(has_position("1 of 3."));
        assert!(!has_position("one of three"));
        assert!(!has_position("43 out of 1000"));
    }
}
