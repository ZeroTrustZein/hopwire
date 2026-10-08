import { useState, type FormEvent } from 'react'
import { api, auth } from '../api/client'
import { errMsg } from '../util'

export default function Login({ onLogin }: { onLogin: () => void }) {
  const [key, setKey] = useState('')
  const [error, setError] = useState<string>()
  const [busy, setBusy] = useState(false)

  async function submit(e: FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(undefined)
    try {
      await api.stats(key.trim())
      auth.set(key.trim())
      onLogin()
    } catch (err) {
      setError(errMsg(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="login-wrap">
      <form className="login-card card" onSubmit={submit}>
        <div className="logo"><span className="logo-icon" />AegisGraph</div>
        <p className="page-subtitle">Investigator console. Enter the pilot API key.</p>
        <input type="password" placeholder="API key" value={key} onChange={(e) => setKey(e.target.value)} autoFocus />
        {error && <div className="error-banner">{error}</div>}
        <button className="primary" disabled={!key.trim() || busy}>{busy ? 'Checking…' : 'Sign in'}</button>
      </form>
    </div>
  )
}
