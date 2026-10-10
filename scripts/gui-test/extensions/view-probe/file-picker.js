// A form with a file picker that takes several files and folders, hidden
// ones too; what is submitted is written down.
const { Action, ActionPanel, Form } = require("@raycast/api");
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
        h(Action.SubmitForm, { title: "Submit", onSubmit: (values) => note("file-picker", values) }),
      ),
    },
    h(Form.FilePicker, { id: "files", title: "Files", allowMultipleSelection: true, canChooseDirectories: true, storeValue: true }),
    h(Form.FilePicker, { id: "one", title: "One File", allowMultipleSelection: false }),
  );
