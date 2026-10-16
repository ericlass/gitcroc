import { expect, test } from "@playwright/test";

// The fake core offers these models for an OpenAI-compatible provider.
test("AI-1 add an OpenAI-compatible provider, pick a fetched model, keep it through AI features off and on", async ({
  page,
}) => {
  await page.goto("/?fake=ai");
  await page.getByRole("button", { name: "Settings" }).click();
  const section = page.getByRole("group", { name: "AI" });
  await expect(section.getByText("No AI provider yet.")).toBeVisible();

  await section.getByRole("button", { name: "Add provider…" }).click();
  const form = section.getByRole("form", { name: "New AI provider" });
  await form.getByLabel("Kind").selectOption({ label: "OpenAI-compatible" });
  await form.getByLabel("Name").fill("Local");
  await form.getByLabel("Base URL").fill("http://localhost:11434/v1");
  await form.getByRole("button", { name: "Fetch models" }).click();
  // No model is preselected.
  const model = form.getByLabel("Model");
  await expect(model).toHaveValue("");
  await model.selectOption("large-model");
  await form.getByRole("button", { name: "Save" }).click();

  const providers = section.getByRole("list", { name: "AI providers" });
  await expect(providers).toContainText("Local");
  await expect(providers).toContainText("large-model");
  await section.getByRole("button", { name: "Set Local as default" }).click();
  await expect(providers).toContainText("Local (default)");

  const features = section.getByRole("checkbox", { name: "AI features" });
  await features.uncheck();
  await expect(providers).toBeHidden();
  await expect(section.getByRole("button")).toHaveCount(0);
  await features.check();
  await expect(providers).toContainText("Local (default)");
  await expect(providers).toContainText("large-model");
});
