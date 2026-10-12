import { render, screen } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import { fakeState, reset } from "./ipc/fake";

afterEach(reset);

test("arch_6_ui_state_comes_only_from_core", async () => {
  fakeState.appInfo = { name: "gitcroc", version: "9.9.9", os: "plan9", arch: "mips" };
  render(() => <App />);
  expect(await screen.findByText("Version 9.9.9 on plan9 (mips)")).toBeDefined();
  expect(screen.getByRole("heading", { name: "gitcroc" })).toBeDefined();
});
