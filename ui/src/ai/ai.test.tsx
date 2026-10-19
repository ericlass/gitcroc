import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "../App";
import { core, type WorkspaceView } from "../ipc";
import { emptyLayout, fakeState, reset } from "../ipc/fake";
import { fakeAi, type FakeProvider } from "../ipc/fakeAi";
import { createAiStore } from "../stores/ai";
import ConsentDialog from "./ConsentDialog";

const KEY = "sk-test-0123456789-SECRET";

afterEach(() => {
  cleanup();
  reset();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Lets several rounds of fake calls finish. */
const settleAll = async () => {
  for (let round = 0; round < 8; round++) {
    await settle();
  }
};

const provider = (id: string, name: string, change: Partial<FakeProvider>): FakeProvider => ({
  id,
  name,
  kind: "openai",
  model: "small-model",
  base_url: "https://api.example.com/v1",
  path: null,
  context_budget: 100_000,
  consent: null,
  ...change,
});

/** The AI store and the consent dialog, as App has them. */
async function consentHost() {
  const ai = createAiStore();
  await ai.subscribe();
  render(() => <ConsentDialog ai={ai} />);
  return ai;
}

test("ai_6_consent_dialog_names_what_is_sent_and_where", async () => {
  fakeAi.providers = [
    provider("p1", "Hosted", {}),
    provider("p2", "Codex", { kind: "codex", model: null, base_url: null, path: "/opt/codex/bin/codex" }),
  ];
  const ai = await consentHost();

  // An API provider names its host and each part of what is sent (AI-5).
  const asked = ai.askConsent("p1");
  const dialog = await screen.findByRole("alertdialog", { name: "Send to api.example.com?" });
  const text = dialog.textContent ?? "";
  expect(text).toContain("sends to the host api.example.com");
  const sent = within(dialog)
    .getAllByRole("listitem")
    .map((item) => item.textContent);
  expect(sent).toEqual([
    "The file's path in the repository",
    "The operation, such as merge, rebase, cherry-pick, revert or stash apply",
    "The labels of the two sides, such as the branch names",
    "Each conflicting part: its base, ours and theirs text, with the 40 lines around it",
    "The messages of up to 10 commits per side that changed the file, each cut at 2,000 characters",
  ]);
  // Cancel has the focus: Enter sends nothing.
  expect(document.activeElement?.textContent).toBe("Cancel");
  fireEvent.click(within(dialog).getByRole("button", { name: "Allow" }));
  expect(await asked).toBe(true);
  expect(fakeAi.providers[0]?.consent).toBe("api.example.com");
  expect(screen.queryByRole("alertdialog")).toBeNull();
  // Asked once: the next use goes on at once.
  expect(await ai.askConsent("p1")).toBe(true);
  expect(screen.queryByRole("alertdialog")).toBeNull();

  // A CLI provider names its program; Cancel sends nothing and keeps nothing.
  const cancelled = ai.askConsent("p2");
  const cli = await screen.findByRole("alertdialog", { name: "Send to /opt/codex/bin/codex?" });
  expect(cli.textContent).toContain("runs the program /opt/codex/bin/codex");
  fireEvent.click(within(cli).getByRole("button", { name: "Cancel" }));
  expect(await cancelled).toBe(false);
  expect(fakeAi.providers[1]?.consent).toBeNull();
  expect(screen.queryByRole("alertdialog")).toBeNull();
});

test("ai_6_consent_asked_again_when_the_host_changes", async () => {
  fakeAi.providers = [provider("p1", "Hosted", { consent: "api.example.com" })];
  const ai = await consentHost();
  expect(await ai.askConsent("p1")).toBe(true);

  await ai.save(
    {
      id: "p1",
      name: "Hosted",
      kind: "openai",
      model: "small-model",
      base_url: "https://router.example.org/api/v1",
      path: null,
      context_budget: null,
    },
    null,
  );
  const asked = ai.askConsent("p1");
  const dialog = await screen.findByRole("alertdialog", { name: "Send to router.example.org?" });
  fireEvent.keyDown(dialog, { key: "Escape" });
  expect(await asked).toBe(false);
  expect(fakeAi.providers[0]?.consent).toBe("api.example.com");
});

test("ai_16_off_hides_the_ai_section_except_the_switch", async () => {
  fakeAi.providers = [provider("p1", "Hosted", {})];
  fakeAi.default = "p1";
  fakeAi.enabled = false;
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  const section = within(await screen.findByRole("group", { name: "AI" }));
  const toggle = section.getByRole("checkbox", { name: "AI features" }) as HTMLInputElement;
  expect(toggle.checked).toBe(false);
  expect(section.getByText(/no AI request is sent and the AI buttons are hidden/)).toBeDefined();
  // The switch is all there is.
  expect(section.getAllByRole("checkbox")).toHaveLength(1);
  expect(section.queryAllByRole("button")).toEqual([]);
  expect(section.queryByRole("list")).toBeNull();
  expect(section.queryByRole("textbox")).toBeNull();

  fireEvent.click(toggle);
  await settleAll();
  expect(fakeAi.enabled).toBe(true);
  const providers = section.getByRole("list", { name: "AI providers" });
  expect(providers.textContent).toContain("Hosted (default)");
  expect(section.getByRole("button", { name: "Add provider…" })).toBeDefined();
  expect(section.getByRole("textbox", { name: /Never send files matching/ })).toBeDefined();
  expect(section.getByRole("checkbox", { name: "AI debug logging" })).toBeDefined();
});

const workspace = (id: string, name: string): WorkspaceView => ({
  id,
  name,
  repos: [],
  groups: [],
  layout: emptyLayout,
  ai: true,
  read_only: false,
  warning: null,
});

/** The AI switch in tab `name`'s menu, or `null`; the menu is closed again. */
async function tabSwitch(name: string) {
  fireEvent.contextMenu(screen.getByRole("tab", { name }));
  const menu = await screen.findByRole("menu", { name: "Tab actions" });
  const item = within(menu).queryByRole("menuitemcheckbox", { name: "AI for this workspace" });
  const checked = item?.getAttribute("aria-checked") ?? null;
  fireEvent.keyDown(document, { key: "Escape" });
  return checked;
}

test("ai_16_off_hides_the_workspace_ai_switch", async () => {
  fakeState.others = [workspace("ws-rust", "Rust")];
  fakeState.tabs = [fakeState.workspace.id, "ws-rust"];
  render(() => <App />);
  await screen.findByRole("tab", { name: "Rust" });
  await settleAll();
  expect(await tabSwitch("My workspace")).toBe("true");
  expect(await tabSwitch("Rust")).toBe("true");

  // The workspace switch (AI-6): off for one tab only, kept in its file.
  fireEvent.contextMenu(screen.getByRole("tab", { name: "Rust" }));
  fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "AI for this workspace" }));
  await settleAll();
  expect(fakeState.others[0]?.ai).toBe(false);
  expect(await tabSwitch("Rust")).toBe("false");
  expect(await tabSwitch("My workspace")).toBe("true");

  // AI features off reaches every open tab at once, from the core's view (ARCH-6).
  await core.commands.aiSettingsSet({ enabled: false, exclude: [], debug_log: false });
  await settleAll();
  expect(await tabSwitch("My workspace")).toBeNull();
  expect(await tabSwitch("Rust")).toBeNull();

  await core.commands.aiSettingsSet({ enabled: true, exclude: [], debug_log: false });
  await settleAll();
  expect(await tabSwitch("Rust")).toBe("false");
});

test("ai_16_off_never_opens_the_consent_dialog", async () => {
  fakeAi.providers = [provider("p1", "Hosted", {})];
  const ai = await consentHost();
  const asked = ai.askConsent("p1");
  await screen.findByRole("alertdialog");
  // Turned off while asking: the question closes unanswered and nothing is kept.
  await ai.setOptions({ enabled: false });
  expect(await asked).toBe(false);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(await ai.askConsent("p1")).toBe(false);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(fakeAi.providers[0]?.consent).toBeNull();
});

test("sec_1_key_field_never_shows_the_key", async () => {
  fakeAi.providers = [provider("p1", "Hosted", { kind: "anthropic", base_url: null })];
  fakeAi.keys = { p1: "an older key" };
  // Every AI payload the core sends the UI.
  const payloads: unknown[] = [];
  await core.events.aiChanged.listen((event) => payloads.push(event.payload));
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  const section = within(await screen.findByRole("group", { name: "AI" }));
  fireEvent.click(section.getByRole("button", { name: "Edit Hosted" }));

  // A stored key shows as such, never as text or a value.
  const form = within(section.getByRole("form", { name: "AI provider Hosted" }));
  expect(form.getByText("Key stored")).toBeDefined();
  const inputs = () => [...document.querySelectorAll("input")].map((input) => input.value);
  expect(inputs()).not.toContain("an older key");

  fireEvent.click(form.getByRole("button", { name: "Replace key…" }));
  const field = form.getByLabelText("API key", { selector: "input" }) as HTMLInputElement;
  expect(field.type).toBe("password");
  expect(field.value).toBe("");
  fireEvent.input(field, { target: { value: KEY } });
  fireEvent.click(form.getByRole("button", { name: "Save" }));
  await settleAll();
  expect(fakeAi.keys.p1).toBe(KEY);

  // Edited again: "Key stored", and the key is nowhere on the page.
  fireEvent.click(section.getByRole("button", { name: "Edit Hosted" }));
  expect(section.getByText("Key stored")).toBeDefined();
  expect(inputs()).not.toContain(KEY);
  expect(document.body.innerHTML).not.toContain(KEY);
  payloads.push(await core.commands.aiProvidersGet());
  expect(payloads.length).toBeGreaterThan(1);
  expect(JSON.stringify(payloads)).not.toContain(KEY);
  expect(JSON.stringify(payloads)).not.toContain("an older key");
});

test("ai_1_the_kind_list_is_the_cores", async () => {
  fakeAi.kinds = ["anthropic", "openai", "claude_code", "codex", "mock"];
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  const section = within(await screen.findByRole("group", { name: "AI" }));
  fireEvent.click(section.getByRole("button", { name: "Add provider…" }));
  const form = within(section.getByRole("form", { name: "New AI provider" }));
  const kind = form.getByLabelText("Kind") as HTMLSelectElement;
  expect([...kind.options].map((option) => option.textContent)).toEqual([
    "Anthropic",
    "OpenAI-compatible",
    "Claude Code",
    "Codex",
    "Mock",
  ]);

  // The mock takes no key, base URL or program.
  fireEvent.change(kind, { target: { value: "mock" } });
  expect(form.queryByRole("group", { name: "API key" })).toBeNull();
  expect(form.queryByLabelText("Base URL")).toBeNull();
  expect(form.queryByLabelText("Program")).toBeNull();
  expect(form.getByLabelText("Model (optional)")).toBeDefined();
});
