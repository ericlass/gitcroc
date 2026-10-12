import { render, screen } from "@solidjs/testing-library";
import { expect, test } from "vitest";
import App from "./App";

test("renders the app name and version", () => {
  render(() => <App />);
  expect(screen.getByRole("heading", { name: "gitcroc" })).toBeDefined();
  expect(screen.getByText(`Version ${__APP_VERSION__}`)).toBeDefined();
});
