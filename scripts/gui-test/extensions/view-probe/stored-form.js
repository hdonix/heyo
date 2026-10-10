// A form whose fields keep their value (`storeValue`): submitted once,
// opened again, they start from what was submitted.
const { Action, ActionPanel, Form, showToast } = require("@raycast/api");
const React = require("react");
const { note } = require("./probe.js");

const h = React.createElement;

exports.default = () =>
  h(
    Form,
    {
      actions: h(
        ActionPanel,
        null,
        h(Action.SubmitForm, {
          title: "Submit",
          onSubmit: (values) => {
            note("stored-form", values);
            showToast({ title: "Submitted", message: JSON.stringify(values) });
          },
        }),
      ),
    },
    h(Form.TextField, { id: "name", title: "Name", placeholder: "A name", storeValue: true }),
    h(
      Form.Dropdown,
      { id: "tone", title: "Tone", storeValue: true },
      h(Form.Dropdown.Item, { value: "warm", title: "Warm" }),
      h(Form.Dropdown.Item, { value: "curt", title: "Curt" }),
      h(Form.Dropdown.Item, { value: "formal", title: "Formal" }),
    ),
    h(Form.Checkbox, { id: "loud", label: "Loud", storeValue: true }),
    h(Form.TextField, { id: "once", title: "Not Kept", placeholder: "Starts empty every time" }),
  );
