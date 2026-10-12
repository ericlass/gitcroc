// Says why the settings are read-only (DATA-1). Paths and reasons are text.
import { Match, Switch } from "solid-js";
import type { SettingsWarning } from "./ipc";

export default function SettingsBanner(props: { warning: SettingsWarning }) {
  return (
    <div role="alert" class="banner">
      <Switch>
        <Match when={props.warning.kind === "newer_schema" && props.warning}>
          {(warning) => (
            <>
              The settings file {warning().path} is from a newer gitcroc (schema {warning().found}; this version knows
              schema {warning().supported}). It is used read-only, so changes are not saved.
            </>
          )}
        </Match>
        <Match when={props.warning.kind === "unreadable" && props.warning}>
          {(warning) => (
            <>
              The settings file {warning().path} cannot be read: {warning().reason}. The defaults are used and the file
              is left as it is; changes are not saved until it is fixed or removed.
            </>
          )}
        </Match>
        <Match when={props.warning.kind === "no_config_dir"}>
          There is no config directory on this system, so settings cannot be saved.
        </Match>
      </Switch>
    </div>
  );
}
