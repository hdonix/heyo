import { List } from "@raycast/api";

const GREETING = "first build";

export default function Hello() {
  console.log(`rendering: ${GREETING}`);
  return (
    <List>
      <List.Item title={`Hello from the ${GREETING}`} />
    </List>
  );
}
