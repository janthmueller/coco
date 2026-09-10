import {
  ArrowRight,
  GitBranch,
  Radio,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import Link from "next/link";

import { site } from "@/lib/site";

const features = [
  {
    description:
      "Send a turn and leave the command. The coordinator keeps the workspace available while you do something else.",
    icon: Terminal,
    title: "Leave work running",
  },
  {
    description:
      "A stable name binds the Codex thread, Git worktree, repository, and settings; active work runs through a dedicated Codex process.",
    icon: GitBranch,
    title: "Return to the exact place",
  },
  {
    description:
      "Inspect state and Linux resource use, answer a request, continue from another terminal, or enter the same work through the native Codex UI.",
    icon: ShieldCheck,
    title: "Stay in control",
  },
  {
    description:
      "Publish validated agent signals, react with local commands, and guard destructive workspace actions.",
    icon: Radio,
    title: "Connect the work",
  },
] as const;

export default function HomePage() {
  return (
    <main className="coco-home flex-1">
      <section className="coco-container coco-hero">
        <div>
          <div className="coco-eyebrow">
            <span className="coco-eyebrow-dot" />
            Codex Coordinator · alpha
          </div>
          <h1>Run Codex work without staying attached.</h1>
          <p className="coco-hero-copy">
            CoCo turns each piece of work into a named workspace that keeps its
            Codex thread, Git worktree, and settings together, with its own
            execution process while active. Start it from one terminal, inspect
            it from another, and return through the native Codex UI.
          </p>
          <div className="coco-actions">
            <Link
              className="coco-button coco-button-primary"
              href="/docs/installation"
            >
              Get started
              <ArrowRight aria-hidden="true" size={16} />
            </Link>
            <Link className="coco-button" href="/docs">
              Read the docs
            </Link>
          </div>
        </div>

        <div className="coco-terminal" aria-label="Example CoCo commands">
          <div className="coco-terminal-bar">
            <span />
            <span />
            <span />
            <div className="coco-terminal-title">~/project</div>
          </div>
          <pre>
            <code>
              <span className="coco-terminal-prompt">$ </span>
              <span className="coco-terminal-command">coco repo add .</span>
              {"\n\n"}
              <span className="coco-terminal-prompt">$ </span>
              <span className="coco-terminal-command">
                coco create fix/login -s &quot;Fix the redirect&quot;
              </span>
              {"\n\n"}
              <span className="coco-terminal-prompt">$ </span>
              <span className="coco-terminal-command">
                coco status fix/login --follow
              </span>
              {"\n"}
              <span className="coco-terminal-prompt">$ </span>
              <span className="coco-terminal-command">coco jump fix/login</span>
            </code>
          </pre>
        </div>
      </section>

      <section className="coco-section">
        <div className="coco-container">
          <div className="coco-section-heading">
            <p className="coco-section-kicker">A local control plane</p>
            <h2>One workspace, several ways to work with it.</h2>
            <p className="coco-section-lead">
              Codex does the coding. CoCo remembers where the work belongs and
              makes the same workspace available to short-lived CLI commands,
              the native terminal UI, and MCP clients.
            </p>
          </div>
          <div className="coco-feature-grid">
            {features.map(({ description, icon: Icon, title }) => (
              <article className="coco-feature-card" key={title}>
                <div className="coco-feature-icon">
                  <Icon aria-hidden="true" size={18} strokeWidth={1.8} />
                </div>
                <h3>{title}</h3>
                <p>{description}</p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <footer className="coco-container coco-footer">
        <span>coco · alpha</span>
        <span className="inline-flex items-center gap-4">
          <Link href="/docs/reference/current-limitations">
            Troubleshooting
          </Link>
          <a href={site.repositoryUrl}>GitHub</a>
        </span>
      </footer>
    </main>
  );
}
