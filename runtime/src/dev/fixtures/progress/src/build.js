import { LiveActivity } from "@heyo/api";

export default async function build() {
  const activity = await LiveActivity.start({ id: "build", name: "Build", icon: "hammer-16" }, { title: "Building" });
  await activity.end({ title: "Built" });
}
