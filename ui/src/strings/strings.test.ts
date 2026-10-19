// @vitest-environment node
// D15: the UI's text comes from src/strings/en.ts. This test reads every
// source file with the TypeScript compiler and reports UI text written
// inline: JSX text with a letter in it, and string literals that reach the
// user through a text prop or parameter (`title`, `aria-label`, `label`, the
// dialog parts' `submit`, …). Text that is data (names, paths, refs, what the
// core sends) is never a literal in the UI, so it is not seen here.
import { readdirSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { expect, test } from "vitest";

const ui = fileURLToPath(new URL("../..", import.meta.url));
const src = join(ui, "src");

/** Props, attributes and parameters whose string reaches the user. */
const TEXT_NAMES = new Set([
  "title",
  "aria-label",
  "aria-description",
  "placeholder",
  "alt",
  "label",
  // The dialog parts' text props (dialogs/parts.tsx).
  "submit",
  "actions",
]);

/** JSX attributes whose string is shown: what `Show` and `Switch` render while their condition is false. */
const JSX_TEXT_NAMES = new Set([...TEXT_NAMES, "fallback"]);

/** Areas not converted yet; M5-05 and M5-06 shrink this list to empty. */
const NOT_CONVERTED = [
  // M5-05
  "ai/",
  "changes/",
  "conflict/",
  "history/",
  "jobs/",
  // M5-06
  "clone/",
  "groups/",
  "integrate/",
  "remote/",
  "reset/",
  "stash/",
  "stores/",
  "sync/",
  "tag/",
];

/** Inline text that stays inline, each with the reason; an entry without `text` holds for its whole file. */
const ALLOWED: { file: string; text?: string; reason: string }[] = [
  { file: "theme/CodeThemePreview.tsx", reason: "it shows sample code, hidden from screen readers, not text" },
  { file: "theme/codeThemes.ts", reason: "code theme names such as GitHub Light are proper names" },
];

/** Files that are not UI: tests, fakes, generated bindings and the strings themselves. */
function isUi(path: string): boolean {
  const name = relative(src, path).split(sep).join("/");
  return (
    /\.tsx?$/.test(name) &&
    !/\.(test|spec)\.tsx?$/.test(name) &&
    !name.endsWith(".d.ts") &&
    !name.startsWith("test/") &&
    !name.startsWith("strings/") &&
    !/^ipc\/fake\w*\.ts$/.test(name) &&
    name !== "ipc/bindings.ts"
  );
}

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? sources(path) : isUi(path) ? [path] : [];
  });
}

/** Whether `text` is UI text: it has a letter, other than in the product name. */
function hasWords(text: string): boolean {
  return /\p{L}/u.test(text.replaceAll("gitcroc", ""));
}

/** Operators whose result can be either side: `a + b`, `a || b`, `a ?? b`; `a && b` can only be `b`. */
const EITHER_SIDE: ts.SyntaxKind[] = [
  ts.SyntaxKind.PlusToken,
  ts.SyntaxKind.BarBarToken,
  ts.SyntaxKind.QuestionQuestionToken,
];

interface Found {
  file: string;
  line: number;
  text: string;
}

/** The inline UI text of `file`. */
function inlineText(file: ts.SourceFile, checker: ts.TypeChecker, name: string): Found[] {
  const found: Found[] = [];
  const report = (node: ts.Node, text: string) => {
    if (hasWords(text)) {
      const { line } = file.getLineAndCharacterOfPosition(node.getStart(file));
      found.push({ file: name, line: line + 1, text: text.trim() });
    }
  };
  // The literals that `node` evaluates to, not those it only compares or passes on.
  const results = (node: ts.Node | undefined): void => {
    if (node === undefined) {
      return;
    }
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
      report(node, node.text);
    } else if (ts.isTemplateExpression(node)) {
      report(node, [node.head.text, ...node.templateSpans.map((span) => span.literal.text)].join("{}"));
      node.templateSpans.forEach((span) => results(span.expression));
    } else if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node) ||
      ts.isNonNullExpression(node) ||
      ts.isJsxExpression(node)
    ) {
      results(node.expression);
    } else if (ts.isConditionalExpression(node)) {
      results(node.whenTrue);
      results(node.whenFalse);
    } else if (ts.isBinaryExpression(node)) {
      const op = node.operatorToken.kind;
      if (EITHER_SIDE.includes(op)) {
        results(node.left);
      }
      if (EITHER_SIDE.includes(op) || op === ts.SyntaxKind.AmpersandAmpersandToken) {
        results(node.right);
      }
    } else if (ts.isArrayLiteralExpression(node)) {
      node.elements.forEach(results);
    } else if (ts.isArrowFunction(node) || ts.isFunctionExpression(node)) {
      if (ts.isBlock(node.body)) {
        returns(node.body);
      } else {
        results(node.body);
      }
    }
  };
  // The returned values of a function body, not of the functions inside it.
  const returns = (node: ts.Node): void => {
    if (ts.isReturnStatement(node)) {
      results(node.expression);
    } else if (!ts.isFunctionLike(node)) {
      ts.forEachChild(node, returns);
    }
  };
  const propName = (name: ts.Node): string | undefined =>
    ts.isIdentifier(name) || ts.isStringLiteral(name) ? name.text : undefined;
  const visit = (node: ts.Node): void => {
    if (ts.isJsxText(node)) {
      report(node, node.text);
    } else if (ts.isJsxExpression(node) && (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent))) {
      results(node.expression);
    } else if (ts.isJsxAttribute(node)) {
      const attribute = ts.isIdentifier(node.name) ? node.name.text : undefined;
      if (attribute !== undefined && JSX_TEXT_NAMES.has(attribute)) {
        results(node.initializer);
      }
    } else if (ts.isPropertyAssignment(node)) {
      const key = propName(node.name);
      if (key !== undefined && TEXT_NAMES.has(key)) {
        results(node.initializer);
      }
    } else if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
      const signature = checker.getResolvedSignature(node);
      node.arguments?.forEach((argument, index) => {
        const parameter = signature?.parameters[index];
        if (parameter !== undefined && TEXT_NAMES.has(parameter.name)) {
          results(argument);
        }
      });
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return found;
}

function options(): ts.CompilerOptions {
  const path = join(ui, "tsconfig.json");
  const config = ts.readConfigFile(path, (file) => ts.sys.readFile(file));
  return { ...ts.parseJsonConfigFileContent(config.config, ts.sys, ui).options, noEmit: true };
}

/** The inline UI text of the files at `paths`, read with `host`. */
function scan(paths: string[], host: ts.CompilerHost = ts.createCompilerHost(options())): Found[] {
  const program = ts.createProgram(paths, options(), host);
  const checker = program.getTypeChecker();
  return paths.flatMap((path) => {
    const file = program.getSourceFile(path);
    expect(file, path).toBeDefined();
    return file === undefined ? [] : inlineText(file, checker, relative(src, path).split(sep).join("/"));
  });
}

test("d15_ui_text_comes_from_the_strings_module", () => {
  const paths = sources(src).filter((path) => {
    const name = relative(src, path).split(sep).join("/");
    return !NOT_CONVERTED.some((area) => name.startsWith(area));
  });
  const found = scan(paths).filter(
    (text) => !ALLOWED.some((allowed) => allowed.file === text.file && (allowed.text ?? text.text) === text.text),
  );
  expect(found.map((text) => `${text.file}:${text.line} ${JSON.stringify(text.text)}`)).toEqual([]);
});

test("d15_the_scan_finds_inline_text_and_lets_data_through", () => {
  // The compiler asks its host for paths with `/`, on Windows too.
  const path = join(src, "layout", "Example.tsx").split(sep).join("/");
  const code = `
    import { Show } from "solid-js";
    const strings = { save: "Save", files: (n: number) => \`\${n} files\` };
    function entry(id: string, label: string) { return { id, label }; }
    function Button(props: { title: string; submit?: string }) {
      return <button title={props.title}>{props.submit}</button>;
    }
    export function Example(props: { name: string; count: number; mode: string }) {
      const menu = [entry("fetch", "Fetch"), entry("pull", strings.save)];
      return (
        <div class={props.mode === "wide" ? "wide" : "narrow"} data-id="pane">
          Inline text
          {props.count > 1 ? "Several" : props.name}
          <Button title="Close tab" submit={props.name} />
          <Button title={strings.save} submit={\`Delete \${props.name}\`} />
          <span aria-label={strings.files(props.count)}>{props.name} · {strings.save} gitcroc …</span>
          <input placeholder={props.name || "Search"} />
          <Show when={props.count} fallback="Nothing">{props.name}</Show>
          {menu.length}
        </div>
      );
    }
  `;
  const base = ts.createCompilerHost(options());
  const host: ts.CompilerHost = {
    ...base,
    fileExists: (file) => file === path || base.fileExists(file),
    readFile: (file) => (file === path ? code : base.readFile(file)),
    getSourceFile: (file, language) =>
      file === path
        ? ts.createSourceFile(file, code, language, true, ts.ScriptKind.TSX)
        : base.getSourceFile(file, language),
  };
  expect(scan([path], host).map((text) => text.text)).toEqual([
    "Fetch",
    "Inline text",
    "Several",
    "Close tab",
    "Delete {}",
    "Search",
    "Nothing",
  ]);
});
