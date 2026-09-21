import React from 'react';
import useBaseUrl from '@docusaurus/useBaseUrl';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
const cards = [{"title": "Connect a gateway", "description": "Create a profile and inspect gateway status.", "path": "quickstart"}, {"title": "Read the command reference", "description": "Find commands, JSON output contracts, and operation-specific limits.", "path": "reference"}, {"title": "Drive it with agents", "description": "MCP transport, LSP for editors, and the agent-skills playbook.", "path": "reference/agent-transports-mcp-ign-mcp-serve-and-lsp-ign-lsp"}, {"title": "Diagnose a connection", "description": "Check profile selection, API authentication, and gateway access.", "path": "troubleshooting"}];
export default function Home(): React.JSX.Element {
 const demosUrl = useBaseUrl('/demos/index.html');
 return <Layout title="ignition-cli" description="Inspect Ignition 8.3+ gateways, manage projects, and automate repeatable tasks with the ign binary.">
  <main><section className="launch-hero"><p className="launch-label">IGNITION / DEVELOPER TOOLS</p><h1>Work with Ignition from the terminal.</h1><p className="lead">Inspect Ignition 8.3+ gateways, manage projects, and automate repeatable tasks with the ign binary.</p>
  <div className="launch-actions"><Link className="button button--primary button--lg" to="/docs/installation">Get started</Link><Link className="button button--outline button--primary button--lg" to="/docs/quickstart">Try a first workflow</Link></div></section>
  <section className="launch-grid" aria-label="Documentation paths">{cards.map(card => <article key={card.path}><h2>{card.title}</h2><p>{card.description}</p><Link to={'/docs/' + card.path}>Read the guide →</Link></article>)}</section>
  <section className="launch-demos" id="demos" aria-label="Recorded walkthroughs"><h2>See it in use</h2><p>Recorded sessions against a fictional batch process. Press play to explore.</p>
    <h3>Ignition CLI</h3><div className="terminal-demo"><iframe className="terminal-demo-frame" src={demosUrl + "?demo=cli"} title="Ignition CLI terminal walkthrough" loading="lazy" allowFullScreen /></div><p>Inspect gateway status, list projects, browse the batch tags, and read temperature and setpoint.</p>
    <h3>Ignition CLI TUI</h3><div className="terminal-demo"><iframe className="terminal-demo-frame" src={demosUrl + "?demo=tui"} title="Ignition CLI TUI terminal walkthrough" loading="lazy" allowFullScreen /></div><p>Navigate the dashboard, browse BlendTank01 tags, inspect a value, and open the batch project.</p>
    <Link to="/docs/demos">Walkthrough details and recording downloads →</Link></section>
  <aside className="launch-maintainer"><p>I’m Patrick Mannion. I work on Ignition development tools and write about the work on FIELDNOTES.</p><p><a href="https://awake-iris-z6ww.here.now/about/">About me</a> · <a href="https://www.linkedin.com/in/mannionpatrick/">LinkedIn</a> · <a href="https://x.com/__pattym__">X</a> · <a href="https://github.com/TheThoughtagen/ignition-cli/graphs/contributors">Project contributors</a></p></aside></main>
 </Layout>;
}
