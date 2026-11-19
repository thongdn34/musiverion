import { useEffect, useState } from "react";
import { GlassPanel } from "@musiverion/ui";
import { commands, type PingReply } from "./bindings";

type PingState =
  { status: "loading" } | { status: "ok"; reply: PingReply } | { status: "error"; message: string };

export function App() {
  const [state, setState] = useState<PingState>({ status: "loading" });

  useEffect(() => {
    commands
      .ping()
      .then((reply) => {
        setState({ status: "ok", reply });
      })
      .catch((err: unknown) => {
        setState({ status: "error", message: String(err) });
      });
  }, []);

  return (
    <main className="backdrop">
      <GlassPanel className="ping">
        <p className="label">Engine</p>
        {state.status === "loading" && <p>Connecting…</p>}
        {state.status === "ok" && (
          <>
            <p>{state.reply.message}</p>
            <p className="secondary">Core {state.reply.coreVersion}</p>
          </>
        )}
        {state.status === "error" && <p className="danger">{state.message}</p>}
      </GlassPanel>
    </main>
  );
}
