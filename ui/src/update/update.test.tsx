import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "../App";
import { fakeState, reset } from "../ipc/fake";
import { CHECKED_AT, defaultUpdateView, fakeUpdate, offered, offerUpdate } from "../ipc/fakeUpdate";
import { createUpdateStore } from "../stores/update";
import { formatChecked } from "./UpdateSection";
import UpdateDialog from "./UpdateDialog";

afterEach(() => {
  cleanup();
  reset();
});

const notice = () => screen.findByRole("button", { name: "gitcroc 0.2.0 is available" });

/** Opens Settings from the toolbar and returns its Updates section. */
async function updatesSection() {
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  const dialog = await screen.findByRole("dialog", { name: "Settings" });
  return within(dialog).getByRole("group", { name: "Updates" });
}

test("PLAT-4 a version on offer shows in the status bar and is announced", async () => {
  offerUpdate(offered, true);
  render(() => <App />);
  const button = await notice();
  expect(button.closest(".status-bar")).not.toBeNull();
  await waitFor(() =>
    expect(document.querySelector("[aria-live=polite]")?.textContent).toBe("gitcroc 0.2.0 is available"),
  );
});

test("PLAT-4 no notice while nothing is on offer", async () => {
  fakeUpdate.view = { ...defaultUpdateView, status: { kind: "up_to_date", checked_at: CHECKED_AT } };
  render(() => <App />);
  expect(await screen.findByRole("button", { description: "Show the job list" })).toBeDefined();
  expect(screen.queryByRole("button", { name: /is available/ })).toBeNull();
});

test("PLAT-4 the dialog of a package that installs offers Install and restart", async () => {
  offerUpdate(offered, true);
  render(() => <App />);
  fireEvent.click(await notice());
  const dialog = await screen.findByRole("dialog", { name: "Update gitcroc" });
  expect(dialog.textContent).toContain("You have gitcroc 0.1.0; gitcroc 0.2.0 is available.");
  expect(within(dialog).queryByRole("button", { name: "Open download page" })).toBeNull();

  fireEvent.click(within(dialog).getByRole("button", { name: "Install and restart" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Update gitcroc" })).toBeNull());
  expect(fakeUpdate.installs).toBe(1);
  // The job's progress, then its end, in the status bar (UI-21).
  const status = screen.getByRole("button", { description: "Show the job list" });
  await waitFor(() => expect(status.textContent).toContain("Update to 0.2.0: Succeeded"));
  expect(fakeUpdate.view.status).toEqual({ kind: "ready_to_restart", version: "0.2.0" });
});

test("PLAT-4 the dialog of a package that only notifies opens the download page", async () => {
  offerUpdate(offered, false);
  render(() => <App />);
  fireEvent.click(await notice());
  const dialog = await screen.findByRole("dialog", { name: "Update gitcroc" });
  expect(dialog.textContent).toContain("This package does not update itself.");
  expect(within(dialog).queryByRole("button", { name: "Install and restart" })).toBeNull();

  fireEvent.click(within(dialog).getByRole("button", { name: "Open download page" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Update gitcroc" })).toBeNull());
  expect(fakeUpdate.pagesOpened).toBe(1);
  expect(fakeUpdate.installs).toBe(0);
});

test("PLAT-4 Later hides the notice", async () => {
  offerUpdate(offered, true);
  render(() => <App />);
  fireEvent.click(await notice());
  const dialog = await screen.findByRole("dialog", { name: "Update gitcroc" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Later" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: /is available/ })).toBeNull());
  expect(fakeUpdate.view.later).toBe(true);
});

test("PLAT-4 the install is refused while another job runs", async () => {
  offerUpdate(offered, true);
  fakeState.jobs = [
    {
      job_id: 7,
      workspace_id: "fake-workspace",
      workspace_name: "My workspace",
      label: "Fetch all remotes",
      repos: [],
      repo_names: [],
      state: "running",
      done: 0,
      total: 0,
      step: null,
      started_at: CHECKED_AT,
      duration_ms: null,
      outcome: null,
      output: "",
      message: null,
      sub_jobs: [],
      report: null,
      ai_files: [],
    },
  ];
  render(() => <App />);
  fireEvent.click(await notice());
  const dialog = await screen.findByRole("dialog", { name: "Update gitcroc" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Install and restart" }));
  expect((await within(dialog).findByRole("alert")).textContent).toBe(
    "wait for the running jobs to end before updating",
  );
  expect(fakeUpdate.installs).toBe(0);
});

test("PLAT-4 unsaved conflict edits ask before the install restarts", async () => {
  offerUpdate(offered, true);
  const update = createUpdateStore();
  void update.subscribe();
  let closed = 0;
  render(() => (
    <UpdateDialog update={update} unsaved={() => ["My workspace: api: src/app.ts"]} onClose={() => closed++} />
  ));
  const dialog = await screen.findByRole("dialog", { name: "Update gitcroc" });
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Install and restart" })).toBeDefined());
  fireEvent.click(within(dialog).getByRole("button", { name: "Install and restart" }));

  const question = await screen.findByRole("alertdialog", { name: "Update and restart?" });
  expect(question.textContent).toContain("My workspace: api: src/app.ts");
  expect(fakeUpdate.installs).toBe(0);
  // Cancel has the focus; cancelling installs nothing.
  expect(document.activeElement?.textContent).toBe("Cancel");
  fireEvent.click(within(question).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(fakeUpdate.installs).toBe(0);

  fireEvent.click(within(dialog).getByRole("button", { name: "Install and restart" }));
  const again = await screen.findByRole("alertdialog", { name: "Update and restart?" });
  fireEvent.click(within(again).getByRole("button", { name: "Install and restart" }));
  await waitFor(() => expect(closed).toBe(1));
  expect(fakeUpdate.installs).toBe(1);
});

test("sec_3_release_notes_render_as_text", async () => {
  offerUpdate({ ...offered, notes: "<img src=x onerror=alert(1)>\nSecond line" }, true);
  render(() => <App />);
  fireEvent.click(await notice());
  const dialog = await screen.findByRole("dialog", { name: "Update gitcroc" });
  const notes = dialog.querySelector(".update-notes");
  expect(notes?.textContent).toBe("<img src=x onerror=alert(1)>\nSecond line");
  expect(dialog.querySelector("img")).toBeNull();
});

test("D12 Settings turns the automatic check off and checks now", async () => {
  fakeUpdate.check = offered;
  render(() => <App />);
  const section = await updatesSection();
  const automatic = within(section).getByRole("checkbox", { name: "Check for updates automatically" });
  expect((automatic as HTMLInputElement).checked).toBe(true);
  expect(section.textContent).toContain("Not checked yet.");

  fireEvent.click(automatic);
  await waitFor(() => expect(fakeState.settings.update_check).toBe(false));
  expect((automatic as HTMLInputElement).checked).toBe(false);

  // Check now runs also while automatic checks are off.
  fireEvent.click(within(section).getByRole("button", { name: "Check now" }));
  await waitFor(() =>
    expect(section.textContent).toContain(`gitcroc 0.2.0 is available (checked ${formatChecked(CHECKED_AT)}).`),
  );
  expect(fakeUpdate.checks).toBe(1);
  expect(await notice()).toBeDefined();
});

test("D12 a failed check shows in Settings with its time, and nowhere else", async () => {
  fakeUpdate.view = {
    ...defaultUpdateView,
    status: { kind: "failed", reason: "error sending request", checked_at: CHECKED_AT },
  };
  render(() => <App />);
  expect(await screen.findByRole("button", { description: "Show the job list" })).toBeDefined();
  expect(screen.queryByRole("alert")).toBeNull();
  const section = await updatesSection();
  expect(section.textContent).toContain(
    `The last check failed at ${formatChecked(CHECKED_AT)}: error sending request`,
  );
  expect(within(section).queryByRole("alert")).toBeNull();
});

test("D12 a build that does not check says so and cannot check now", async () => {
  fakeUpdate.view = { ...defaultUpdateView, checks: false };
  render(() => <App />);
  const section = await updatesSection();
  expect(section.textContent).toContain("This build does not check for updates; only release builds do.");
  expect((within(section).getByRole("button", { name: "Check now" }) as HTMLButtonElement).disabled).toBe(true);
});

test("SEC-7 read-only settings keep the automatic check as it is", async () => {
  fakeState.settings = { ...fakeState.settings, read_only: true };
  render(() => <App />);
  const section = await updatesSection();
  const automatic = within(section).getByRole("checkbox", { name: "Check for updates automatically" });
  expect((automatic as HTMLInputElement).disabled).toBe(true);
  // Checking needs no write, so it stays possible.
  expect((within(section).getByRole("button", { name: "Check now" }) as HTMLButtonElement).disabled).toBe(false);
});
