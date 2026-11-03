/// <reference types="vite/client" />
import helpHtml from './help.html?raw'

// help.html is edited by the owner and compiled into the app; it is never
// loaded from the network or from user data.
export function HelpPanel() {
  return (
    <section
      className="vault-tab-panel settings-tab-panel"
      id="panel-help"
      role="tabpanel"
      aria-labelledby="tab-help"
    >
      <div className="settings-heading">
        <h1>Help</h1>
      </div>
      <section className="panel settings-panel help-content" dangerouslySetInnerHTML={{ __html: helpHtml }} />
    </section>
  )
}
