import React from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
const cards = [{"title": "Connect a gateway", "description": "Create a profile and inspect gateway status.", "path": "quickstart"}, {"title": "Read the command reference", "description": "Find commands, JSON output contracts, and operation-specific limits.", "path": "reference"}, {"title": "Diagnose a connection", "description": "Check profile selection, API authentication, and gateway access.", "path": "troubleshooting"}];
export default function Home(): React.JSX.Element {
 return <Layout title="ignition-cli" description="Inspect Ignition 8.3+ gateways, manage projects, and automate repeatable tasks with the ign binary.">
  <main><section className="launch-hero"><p className="launch-label">IGNITION / DEVELOPER TOOLS</p><h1>Work with Ignition from the terminal.</h1><p className="lead">Inspect Ignition 8.3+ gateways, manage projects, and automate repeatable tasks with the ign binary.</p>
  <div className="launch-actions"><Link className="button button--primary button--lg" to="/docs/installation">Get started</Link><Link className="button button--outline button--primary button--lg" to="/docs/quickstart">Try a first workflow</Link></div></section>
  <section className="launch-grid" aria-label="Documentation paths">{cards.map(card => <article key={card.path}><h2>{card.title}</h2><p>{card.description}</p><Link to={'/docs/' + card.path}>Read the guide →</Link></article>)}</section>
  <aside className="launch-maintainer"><p>I’m Patrick Mannion. I work on Ignition development tools and write about the work on FIELDNOTES.</p><p><a href="https://awake-iris-z6ww.here.now/about/">About me</a> · <a href="https://www.linkedin.com/in/mannionpatrick/">LinkedIn</a> · <a href="https://x.com/__pattym__">X</a> · <a href="https://github.com/TheThoughtagen/ignition-cli/graphs/contributors">Project contributors</a></p></aside></main>
 </Layout>;
}
