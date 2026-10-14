// Clone for the fake core (fake.ts): the default folder name and the target
// checks of the core's `clone_default_name` and `clone_target_check`,
// simplified. Test support only; the real rules are the core's.

/** What is on the fake disk at a path: an empty folder, a folder with files, or a file. */
export type FakeFolder = "empty" | "full" | "file";

/** The last path segment of `url` without `.git`, or `null`. */
export function fakeDefaultName(url: string): string | null {
  let path = url.trim().replace(/[/\\]+$/, "");
  path = path.endsWith("/.git") ? path.slice(0, -"/.git".length) : path;
  const last = path.split(/[/\\:]/).pop() ?? "";
  const name = last.endsWith(".git") ? last.slice(0, -".git".length) : last;
  return fakeNameRefusal(name) === null ? name : null;
}

function fakeNameRefusal(name: string): string | null {
  if (name === "") {
    return "the folder name is empty";
  }
  if (name === "." || name === "..") {
    return "the folder name cannot be . or ..";
  }
  return /[/\\]/.test(name) ? "the folder name cannot contain / or \\" : null;
}

/** Why a clone into folder `name` of `parent` is refused on the fake disk `folders`, or `null`. */
export function fakeTargetRefusal(folders: Record<string, FakeFolder>, parent: string, name: string): string | null {
  const target = `${parent}/${name}`;
  const found = folders[target];
  return (
    fakeNameRefusal(name) ??
    (found === "full"
      ? `the folder ${target} exists and is not empty`
      : found === "file"
        ? `${target} exists and is not a folder`
        : null)
  );
}
