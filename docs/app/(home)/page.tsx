import { ArrowRight, GitBranch, ShieldCheck, Terminal } from "lucide-react";
import Link from "next/link";

import { site } from "@/lib/site";

const features = [
  {
    description:
      "Each task works in a separate Git checkout, so your current branch stays untouched.",
    icon: GitBranch,
    title: "Keep changes separate",
  },
  {
    description:
      "List active tasks, follow Codex, send a follow-up, and inspect the resulting diff from the terminal.",
    icon: Terminal,
    title: "Stay in control",
  },
  {
    description:
      "Other applications can inspect CoCo through MCP. Sending more work stays disabled unless you enable it.",
    icon: ShieldCheck,
    title: "Connect safely",
  },
] as const;

export default function HomePage() {
  return (
    <main className="coco-home flex-1">
      <section className="coco-container coco-hero">
        <div>
          <div className="coco-eyebrow">
            <span className="coco-eyebrow-dot" />
            Codex Coordinator · early preview
          </div>
          <h1>Run Codex tasks side by side.</h1>
          <p className="coco-hero-copy">
            CoCo gives every task its own Git worktree and keeps its Codex
            conversation and changes together. Start another task without
            disturbing the branch you are using now.
          </p>
          <div className="coco-actions">
            <Link
              className="coco-button coco-button-primary"
              href="/docs/getting-started"
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
                coco new fix-login --base main \
              </span>
              {"\n"} --context fresh \{"\n"} --goal &quot;Fix the login
              redirect&quot;
              {"\n\n"}
              <span className="coco-terminal-prompt">$ </span>
              <span className="coco-terminal-command">
                coco watch fix-login
              </span>
              {"\n"}
              <span className="coco-terminal-prompt">$ </span>
              <span className="coco-terminal-command">coco diff fix-login</span>
            </code>
          </pre>
        </div>
      </section>

      <section className="coco-section">
        <div className="coco-container">
          <div className="coco-section-heading">
            <p className="coco-section-kicker">A calmer parallel workflow</p>
            <h2>Give each task its own place to work.</h2>
            <p className="coco-section-lead">
              CoCo handles the separate checkout and remembers which Codex
              conversation belongs to it. You decide what to start, continue,
              and keep.
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
        <span>coco · early preview</span>
        <span className="inline-flex items-center gap-4">
          <Link href="/docs/reference/current-limitations">
            Current limitations
          </Link>
          <a href={site.repositoryUrl}>GitHub</a>
        </span>
      </footer>
    </main>
  );
}
