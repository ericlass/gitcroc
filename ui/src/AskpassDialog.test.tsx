import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import type { AskpassPrompt } from "./ipc";
import { emitAskpassClosed, emitAskpassPrompt, fakeState, reset } from "./ipc/fake";

afterEach(() => {
  cleanup();
  reset();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const prompt = (prompt_id: number, kind: AskpassPrompt["kind"], text: string): AskpassPrompt => ({
  prompt_id,
  kind,
  text,
  workspace_name: "Platform",
  repo_names: ["api"],
  job_label: "Fetch",
});

async function app() {
  render(() => <App />);
  await screen.findByRole("toolbar", { name: "Main" });
  await settle();
}

test("D10 askpass dialog masks a password, asks a host key as yes or no, and Escape only cancels", async () => {
  await app();

  // A password prompt, named by its job and repo, masked as it is typed.
  emitAskpassPrompt(prompt(1, "secret", "Password for 'https://alice@git.example.com': "));
  const dialog = await screen.findByRole("dialog", { name: "Fetch: api in Platform" });
  const field = within(dialog).getByLabelText("Password for 'https://alice@git.example.com':");
  expect(field.getAttribute("type")).toBe("password");
  fireEvent.input(field, { target: { value: "s3cret" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
  await settle();
  expect(fakeState.askpassAnswers).toEqual([{ promptId: 1, answer: "s3cret" }]);
  expect(screen.queryByRole("dialog")).toBeNull();

  // An SSH host key question is a yes or no choice, with no text field.
  emitAskpassPrompt(
    prompt(2, "yes_no", "The authenticity of host 'git.example.com' can't be established.\nAre you sure you want to continue connecting (yes/no/[fingerprint])? "),
  );
  const hostKey = await screen.findByRole("dialog");
  expect(within(hostKey).queryByRole("textbox")).toBeNull();
  expect(hostKey.textContent).toContain("can't be established.");
  fireEvent.click(within(hostKey).getByRole("button", { name: "Yes" }));
  await settle();
  expect(fakeState.askpassAnswers.at(-1)).toEqual({ promptId: 2, answer: "yes" });

  // Escape sends the cancel and nothing typed so far.
  emitAskpassPrompt(prompt(3, "text", "Username for 'https://git.example.com': "));
  const username = await screen.findByRole("dialog");
  const name = within(username).getByRole("textbox");
  expect(name.getAttribute("type")).toBe("text");
  fireEvent.input(name, { target: { value: "half typed" } });
  fireEvent.keyDown(username, { key: "Escape" });
  await settle();
  expect(fakeState.askpassAnswers.at(-1)).toEqual({ promptId: 3, answer: null });
  expect(fakeState.askpassAnswers.map((sent) => sent.answer)).not.toContain("half typed");
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("D10 a prompt withdrawn by the core closes, and the next one shows", async () => {
  await app();
  emitAskpassPrompt(prompt(1, "text", "Username for 'https://a.example.com': "));
  emitAskpassPrompt(prompt(2, "text", "Username for 'https://b.example.com': "));
  expect((await screen.findByRole("dialog")).textContent).toContain("a.example.com");

  emitAskpassClosed({ prompt_id: 1 });
  expect((await screen.findByRole("dialog")).textContent).toContain("b.example.com");
  emitAskpassClosed({ prompt_id: 2 });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fakeState.askpassAnswers).toEqual([]);
});
