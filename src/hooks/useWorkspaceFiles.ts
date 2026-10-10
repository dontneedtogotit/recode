import { useCallback, useState } from 'react';
import {
  listProjectFiles,
  readProjectFile,
  writeProjectFile,
  validateWorkspacePath,
  type ProjectFile,
  type PathValidation,
} from '../lib/ipc';

export function useWorkspaceFiles(workspaceId: string | null) {
  const [files, setFiles] = useState<ProjectFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!workspaceId) return;
    setLoading(true);
    setError(null);
    try {
      const result = await listProjectFiles(workspaceId);
      setFiles(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'failed to list files');
    } finally {
      setLoading(false);
    }
  }, [workspaceId]);

  const read = useCallback(
    async (path: string) => {
      if (!workspaceId) return null;
      setError(null);
      try {
        return await readProjectFile(workspaceId, path);
      } catch (e) {
        setError(e instanceof Error ? e.message : 'failed to read file');
        return null;
      }
    },
    [workspaceId],
  );

  const write = useCallback(
    async (file: ProjectFile) => {
      if (!workspaceId) return false;
      setError(null);
      try {
        await writeProjectFile(workspaceId, file);
        await refresh();
        return true;
      } catch (e) {
        setError(e instanceof Error ? e.message : 'failed to write file');
        return false;
      }
    },
    [workspaceId, refresh],
  );

  const validate = useCallback(
    async (path: string): Promise<PathValidation> => {
      if (!workspaceId) {
        return { valid: false, normalized: '', issues: ['No workspace selected'] };
      }
      setError(null);
      try {
        return await validateWorkspacePath(workspaceId, path);
      } catch (e) {
        setError(e instanceof Error ? e.message : 'validation failed');
        return { valid: false, normalized: '', issues: ['validation failed'] };
      }
    },
    [workspaceId],
  );

  return {
    files,
    loading,
    error,
    refresh,
    read,
    write,
    validate,
  };
}
