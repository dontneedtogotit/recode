import { useState, useEffect, useRef, useCallback } from 'react';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { Row } from '../components/Row';
import { listAgents } from '../lib/agentRegistry';
import { executeAgentTask } from '../lib/ipc';
import { loadAgentHistory } from '../lib/agentHistory';

type AgentChatViewProps = {
  onOpenConsole: () => void;
};

type ChatMessage = {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  ts: number;
  agentId?: string;
};

type AgentEntry = {
  id: string;
  name: string;
  role: string;
  description: string;
  capabilities: string[];
};

type HistoryRecord = {
  id: string;
  agentId: string;
  input: string;
  success: boolean;
  output?: string;
  error?: string;
  durationMs?: number;
  timestamp: number;
};

export default function AgentChatView({ onOpenConsole }: AgentChatViewProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([
    { id: 'init', role: 'system', content: 'Agent chat is ready. Ask the runtime agent to perform a task.', ts: Date.now() },
  ]);
  const [input, setInput] = useState('');
  const [agentId, setAgentId] = useState('planner');
  const [agents, setAgents] = useState<AgentEntry[]>([]);
  const [sending, setSending] = useState(false);
  const [pendingResponse, setPendingResponse] = useState(false);
  const [history, setHistory] = useState<HistoryRecord[]>([]);
  const [showHistory, setShowHistory] = useState(false);
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const nextIdRef = useRef(1);

  useEffect(() => {
    let cancelled = false;
    listAgents().then((items) => {
      if (!cancelled) setAgents(items as AgentEntry[]);
    });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const loaded = loadAgentHistory();
    setHistory(loaded.records);
  }, []);

  useEffect(() => {
    const node = bottomRef.current;
    if (node && typeof node.scrollIntoView === 'function') {
      node.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages]);

  const append = useCallback((message: ChatMessage) => {
    setMessages((current) => [...current, message]);
  }, []);

  const handleSend = useCallback(async () => {
    const trimmed = input.trim();
    if (!trimmed || sending) return;
    const selectedAgentId = agentId;
    const messageId = `msg-${nextIdRef.current++}`;
    append({ id: messageId, role: 'user', content: trimmed, ts: Date.now(), agentId: selectedAgentId });
    setInput('');
    setSending(true);
    setPendingResponse(true);
    try {
      const result = await executeAgentTask({ agentId: selectedAgentId, input: trimmed });
      const content = result.success
        ? typeof result.output === 'string' ? result.output : JSON.stringify(result.output)
        : `Agent ${selectedAgentId} failed: ${result.error ?? 'unknown error'}`;
      append({ id: `${messageId}-response`, role: 'assistant', content, ts: Date.now(), agentId: selectedAgentId });
      // Refresh history after task completes
      const loaded = loadAgentHistory();
      setHistory(loaded.records);
    } catch (error) {
      append({ id: `${messageId}-response`, role: 'assistant', content: `Agent ${selectedAgentId} failed: ${error instanceof Error ? error.message : 'unknown error'}`, ts: Date.now(), agentId: selectedAgentId });
    } finally {
      setSending(false);
      setPendingResponse(false);
    }
  }, [agentId, append, input, sending]);

  const selectedAgent = agents.find((item) => item.id === agentId);

  return (
    <div className="view-enter" style={{ display: 'flex', flexDirection: 'column', gap: 14, height: '100%' }}>
      <div className="page-header">
        <div>
          <h1 className="page-title">Agent chat</h1>
          <p className="page-subtitle">Inspect registered agent capabilities and local task history.</p>
        </div>
        <div className="flex-start" style={{ gap: 8 }}>
          <button type="button" onClick={onOpenConsole} className="secondary">Open console</button>
          <button type="button" onClick={() => setShowHistory(!showHistory)} className="secondary">
            {showHistory ? 'Hide history' : 'Show history'}
          </button>
        </div>
      </div>

      <div className="status-pill warn" role="note">Runtime limitation: local agent tasks currently return simulated acknowledgments. A successful task is not evidence of generated files, code review, or a provider-backed conversation. Use Project for the file-generation path.</div>

      <Card style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 12, minHeight: 0 }}>
        {selectedAgent && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <div style={{ fontWeight: 700 }}>{selectedAgent.name}</div>
            <div style={{ fontSize: 13, color: 'var(--text-secondary)' }}>{selectedAgent.description}</div>
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              {selectedAgent.capabilities.map((capability) => (
                <span key={capability} className="badge info" style={{ fontSize: 11, padding: '3px 8px' }}>
                  {capability}
                </span>
              ))}
            </div>
          </div>
        )}

        {showHistory && (
          <div className="chat-history-panel" role="region" aria-label="Task history">
            <div className="text-xs text-muted">Task history</div>
            {history.length === 0 ? (
              <div className="text-xs text-muted">Completed tasks are saved locally after the first run.</div>
            ) : history.slice(0, 20).map((record) => (
              <div key={record.id} className="chat-history-item">
                <div className="flex-between">
                  <span className="font-semibold">{record.agentId}</span>
                  <span className={`badge ${record.success ? 'ok' : 'err'}`}>{record.success ? 'Success' : 'Failed'}</span>
                </div>
                <div className="text-xs text-secondary">Input: {record.input.length > 80 ? `${record.input.slice(0, 80)}…` : record.input}</div>
                {record.output && <div className="text-xs text-success">Output: {record.output.length > 80 ? `${record.output.slice(0, 80)}…` : record.output}</div>}
                {record.error && <div className="text-xs text-danger">Error: {record.error}</div>}
                <div className="text-xs text-muted">{record.durationMs ? `${record.durationMs}ms` : ''} · {new Date(record.timestamp).toLocaleString()}</div>
              </div>
            ))}
          </div>
        )}

        <div className="chat-log" role="log" aria-live="polite" aria-label="Agent conversation">
          {messages.map((message) => (
            <div key={message.id} style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <div className="chat-message-meta">
                <span>{message.role === 'user' ? 'You' : message.role === 'assistant' ? `Agent ${message.agentId ?? agentId}` : 'System'}</span>
                <span> · {new Date(message.ts).toLocaleTimeString()}</span>
              </div>
              <div className={`chat-bubble ${message.role}`}>{message.content}</div>
            </div>
          ))}
          {pendingResponse && (
            <div className="chat-message-pending">
              <div className="chat-message-meta">Agent · thinking</div>
              <div className="chat-bubble assistant chat-thinking" aria-label="Agent is thinking">
                <span className="thinking-dot" />
                <span className="thinking-dot" />
                <span className="thinking-dot" />
              </div>
            </div>
          )}
          <div ref={bottomRef} />
        </div>

        <div className="chat-composer">
          <Row>
            <Field label="Agent" htmlFor="agent-chat-select">
              <select id="agent-chat-select" value={agentId} onChange={(event) => setAgentId(event.target.value)}>
                {agents.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Message" htmlFor="agent-chat-input" style={{ flex: 1, minWidth: 260 }}>
              <textarea
                id="agent-chat-input"
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    handleSend();
                  }
                }}
                placeholder="Ask the agent to do something... (Shift+Enter for newline)"
                rows={2}
                style={{ minHeight: 56, resize: 'vertical' }}
              />
            </Field>
            <button type="button" onClick={handleSend} disabled={sending}>
              {sending ? 'Sending...' : 'Send'}
            </button>
          </Row>
          <div className="text-xs text-muted">Enter sends · Shift+Enter adds a newline · Conversation history is saved locally</div>
        </div>
      </Card>
    </div>
  );
}