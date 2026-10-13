//! Forms Heyo shows before a command runs: preferences and arguments.

import { Action, ActionPanel, Form } from "../api/components";
import { showToast, Toast } from "../api/system";
import { t } from "../../../shared/i18n";
import {
  preferenceValues,
  savePreferences,
  storedPreferences,
  type CommandManifest,
  type Extension,
  type Preference,
} from "../context";

function PreferenceField({ pref, value }: { pref: Preference; value: unknown }) {
  const common = {
    id: pref.name,
    title: pref.title ?? pref.name,
    info: pref.description,
    placeholder: pref.placeholder,
  };
  switch (pref.type) {
    case "checkbox":
      return <Form.Checkbox {...common} label={pref.label ?? pref.title ?? ""} defaultValue={value === true} />;
    case "dropdown":
      return (
        <Form.Dropdown {...common} defaultValue={value as string}>
          {(pref.data ?? []).map((d) => (
            <Form.Dropdown.Item key={d.value} value={d.value} title={d.title} />
          ))}
        </Form.Dropdown>
      );
    case "password":
      return <Form.PasswordField {...common} defaultValue={(value as string) ?? ""} />;
    default:
      return <Form.TextField {...common} defaultValue={value == null ? "" : String(value)} />;
  }
}

export function PreferencesForm(props: { extension: Extension; command?: string; onSaved(): void }) {
  const { manifest } = props.extension;
  const extensionPrefs = manifest.preferences ?? [];
  const commandPrefs = manifest.commands.find((c) => c.name === props.command)?.preferences ?? [];
  const values = preferenceValues(props.extension, props.command);

  const submit = (submitted: Record<string, unknown>) => {
    const missing = [...extensionPrefs, ...commandPrefs].filter(
      (p) => p.required && (submitted[p.name] === undefined || submitted[p.name] === ""),
    );
    if (missing.length) {
      showToast({
        style: Toast.Style.Failure,
        title: t("Required preferences missing"),
        message: missing.map((p) => p.title ?? p.name).join(", "),
      });
      return;
    }
    const stored = storedPreferences(manifest.name);
    for (const p of extensionPrefs) stored.extension[p.name] = submitted[p.name];
    if (props.command && commandPrefs.length) {
      stored.commands[props.command] = Object.fromEntries(commandPrefs.map((p) => [p.name, submitted[p.name]]));
    }
    savePreferences(manifest.name, stored);
    props.onSaved();
  };

  return (
    <Form
      navigationTitle={t("{0} Preferences", manifest.title)}
      actions={
        <ActionPanel>
          <Action.SubmitForm title={t("Save Preferences")} onSubmit={submit} />
        </ActionPanel>
      }
    >
      <Form.Description text={t("Configure {0}. Required fields are marked.", manifest.title)} />
      {[...extensionPrefs, ...commandPrefs].map((pref) => (
        <PreferenceField key={pref.name} pref={{ ...pref, title: `${pref.title ?? pref.name}${pref.required ? " *" : ""}` }} value={values[pref.name]} />
      ))}
    </Form>
  );
}

export function ArgumentsForm(props: { command: CommandManifest; onSubmit(values: Record<string, string>): void }) {
  const args = props.command.arguments ?? [];

  const submit = (values: Record<string, string>) => {
    const missing = args.filter((a) => a.required && !values[a.name]);
    if (missing.length) {
      showToast({
        style: Toast.Style.Failure,
        title: t("Required arguments missing"),
        message: missing.map((a) => a.placeholder ?? a.name).join(", "),
      });
      return;
    }
    props.onSubmit(values);
  };

  return (
    <Form
      navigationTitle={props.command.title}
      actions={
        <ActionPanel>
          <Action.SubmitForm title={t("Run {0}", props.command.title)} onSubmit={submit} />
        </ActionPanel>
      }
    >
      {args.map((arg, i) => {
        const common = { id: arg.name, title: arg.placeholder ?? arg.name, autoFocus: i === 0 };
        if (arg.type === "dropdown") {
          return (
            <Form.Dropdown key={arg.name} {...common}>
              {(arg.data ?? []).map((d) => (
                <Form.Dropdown.Item key={d.value} value={d.value} title={d.title} />
              ))}
            </Form.Dropdown>
          );
        }
        if (arg.type === "password") return <Form.PasswordField key={arg.name} {...common} placeholder={arg.placeholder} />;
        return <Form.TextField key={arg.name} {...common} placeholder={arg.placeholder} />;
      })}
    </Form>
  );
}
