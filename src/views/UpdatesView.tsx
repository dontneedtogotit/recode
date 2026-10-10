import { useState } from 'react';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { Row } from '../components/Row';
import { checkUpdates, verifyUpdateSignature, type UpdateCheckResult } from '../hooks/useUpdater';

export default function UpdatesView() {
  const [status, setStatus] = useState<string | null>(null);
  const [artifactPath, setArtifactPath] = useState('');
  const [signaturePath, setSignaturePath] = useState('');
  const [publicKeyPath, setPublicKeyPath] = useState('');
  const [manifest, setManifest] = useState<UpdateCheckResult['manifest'] | null>(null);
  const [lastChecked, setLastChecked] = useState<Date | null>(null);
  const [verificationResult, setVerificationResult] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const pathsReady = Boolean(artifactPath.trim() && signaturePath.trim() && publicKeyPath.trim());

  const handleCheck = async () => {
    if (checking) return;
    setChecking(true);
    setStatus('Checking for updates...');
    setManifest(null);
    setVerificationResult(null);
    try {
      setLastChecked(new Date());
      const result = await checkUpdates('https://example.com/updater', '0.1.0');
      if (result.available) {
        setStatus(`Update available`);
        setManifest(result.manifest ?? null);
      } else {
        setStatus('No updates available');
      }
    } catch (error) {
      setStatus(`Update check failed: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setChecking(false);
    }
  };

  const handleVerify = async () => {
    if (!pathsReady || verifying) return;
    setVerifying(true);
    setVerificationResult('Verifying...');
    try {
      const valid = await verifyUpdateSignature(artifactPath, signaturePath, publicKeyPath);
      setVerificationResult(valid ? 'Signature verified' : 'Signature verification failed');
    } catch (error) {
      setVerificationResult(`Verification failed: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setVerifying(false);
    }
  };

  return (
    <div className="view-enter" style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div className="page-header">
        <h1 className="page-title" style={{ margin: 0 }}>Updates</h1>
        {manifest?.version ? <div className="badge info">Version {manifest.version}</div> : null}
      </div>
      <p className="page-subtitle">Check for updates and verify release signatures.</p>

      <Card style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Row>
          <button type="button" onClick={handleCheck} disabled={checking}>Check for updates</button>
          {status ? <div className="status-pill">{status}</div> : null}
          {lastChecked ? <div className="text-xs text-muted">Last checked {lastChecked.toLocaleTimeString()}</div> : null}
        </Row>

        {manifest && (
          <div className="card card-flat" style={{ padding: 14, display: 'flex', flexDirection: 'column', gap: 6 }}>
            <div style={{ fontWeight: 700 }}>Manifest</div>
            <div style={{ color: 'var(--text-secondary)', fontSize: 13 }}>Version: {manifest.version}</div>

          </div>
        )}

        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div style={{ fontWeight: 700 }}>Signature verification</div>
          <Row>
            <Field label="Artifact path">
              <input value={artifactPath} onChange={(e) => setArtifactPath(e.target.value)} placeholder="Artifact path" />
            </Field>
            <Field label="Signature path">
              <input value={signaturePath} onChange={(e) => setSignaturePath(e.target.value)} placeholder="Signature path" />
            </Field>
          </Row>
          <Row>
            <Field label="Public key path">
              <input value={publicKeyPath} onChange={(e) => setPublicKeyPath(e.target.value)} placeholder="Public key path" />
            </Field>
            <button type="button" onClick={handleVerify} disabled={!pathsReady || verifying}>Verify signature</button>
          </Row>
          {verificationResult && <div className="status-pill">{verificationResult}</div>}
        </div>
      </Card>
    </div>
  );
}
