import { useGatewayRun } from '../hooks/useGatewayRun';
import { Card } from '../components/Card';
import { EmptyState } from '../components/EmptyState';
import { SectionHeader } from '../components/SectionHeader';
import { Field } from '../components/Field';
import { Row } from '../components/Row';

export default function AIView() {
  const {
    provider,
    taskType,
    prompt,
    response,
    status,
    loading,
    rawResponse,
    setProvider,
    setTaskType,
    setPrompt,
    run,
  } = useGatewayRun();

  const statusVariant = status === 'Open' || status === 'Closed' ? 'ok' : status === 'HalfOpen' ? 'warn' : status === null ? 'info' : 'err';

  return (
    <div className="view-enter stack-lg">
      <div className="page-header">
        <h1 className="page-title" style={{ margin: 0 }}>AI gateway</h1>
        <div className={`badge ${statusVariant}`}>{status ?? '—'}</div>
      </div>
      <p className="page-subtitle">Send a task through the configured OpenAI-compatible gateway and inspect provider responses.</p>
      <p className="text-xs text-muted">Set RECODE_OPENAI_BASE_URL and RECODE_OPENAI_MODEL before launching the app; remote HTTPS endpoints also require RECODE_OPENAI_API_KEY. Settings provider entries do not configure this gateway.</p>

      <Card style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Row>
          <Field label="Provider">
            <input value={provider} onChange={(e) => setProvider(e.target.value)} placeholder="openai-compatible" />
          </Field>
          <Field label="Task type">
            <input value={taskType} onChange={(e) => setTaskType(e.target.value)} placeholder="chat" />
          </Field>
        </Row>
        <Row>
          <Field label="Prompt">
            <input value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="Hello, gateway" />
          </Field>
        </Row>
        <Row>
          <button type="button" onClick={run} disabled={loading}>
            {loading ? 'Running...' : 'Run gateway task'}
          </button>
        </Row>

        {response ? (
          <div className="stack-sm">
            <SectionHeader title="Response" description="Provider response returned by the gateway." />
            <div className="response-output">
              <div className="response-output-toolbar">
                <span className="text-xs text-muted">Structured response</span>
                <button type="button" className="ghost compact-button" onClick={() => void navigator.clipboard?.writeText(JSON.stringify(response, null, 2))}>Copy JSON</button>
              </div>
              <div className="diff-placeholder">{JSON.stringify(response, null, 2)}</div>
            </div>
          </div>
        ) : (
          <EmptyState title="No response yet" description="Run a gateway task to inspect a structured provider response." />
        )}

        {rawResponse && (
          <div className="stack-sm">
            <div className="section-header-title">Raw response</div>
            <div className="diff-placeholder">{rawResponse}</div>
          </div>
        )}
      </Card>
    </div>
  );
}
