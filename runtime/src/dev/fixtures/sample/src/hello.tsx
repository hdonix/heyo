import { Action, ActionPanel, List } from "@raycast/api";
import { useState } from "react";

import { greetings } from "./lib/greeting";

export default function Hello() {
  const [count, setCount] = useState(0);
  return (
    <List searchBarPlaceholder={`Chosen ${count} times`}>
      {greetings.map((greeting) => (
        <List.Item
          key={greeting}
          title={greeting}
          actions={
            <ActionPanel>
              <Action title="Choose" onAction={() => setCount(count + 1)} />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}
