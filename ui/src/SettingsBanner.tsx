// Says why the settings are read-only (DATA-1). Paths and reasons are text.
import { Match, Switch } from "solid-js";
import type { ConfigWarning } from "./ipc";
import { strings } from "./strings/en";

export default function SettingsBanner(props: { warning: ConfigWarning }) {
  return (
    <div role="alert" class="banner">
      <Switch>
        <Match when={props.warning.kind === "newer_schema" && props.warning}>
          {(warning) => (
            <>{strings.settings.newerSchema(warning().path, warning().found, warning().supported)}</>
          )}
        </Match>
        <Match when={props.warning.kind === "unreadable" && props.warning}>
          {(warning) => (
            <>{strings.settings.unreadable(warning().path, warning().reason)}</>
          )}
        </Match>
        <Match when={props.warning.kind === "no_config_dir"}>
          {strings.settings.noConfigDir}
        </Match>
      </Switch>
    </div>
  );
}
