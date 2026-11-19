import React from 'react';
import ReactDOM from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import App from './App';
import './index.css';

function getErrorStatus(error: unknown): number | undefined {
  return (error as { status?: number } | null)?.status;
}

function getRetryDelay(attemptIndex: number, error: unknown): number {
  const retryAfter = (error as { retryAfter?: number } | null)?.retryAfter;
  if (typeof retryAfter === 'number' && Number.isFinite(retryAfter) && retryAfter > 0) {
    // Respect gateway Retry-After (seconds), capped to avoid long stalls.
    return Math.min(retryAfter * 1000, 30000);
  }
  // Exponential backoff with cap for transient failures.
  return Math.min(1000 * 2 ** attemptIndex, 15000);
}

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: (failureCount, error) => {
        const status = getErrorStatus(error);
        // Don't retry client/auth errors (except rate limiting / transient timeouts).
        if (status === 401 || status === 403 || status === 404) return false;
        if (status === 408 || status === 429 || status === 503 || status === 502 || status === 504) {
          return failureCount < 4;
        }
        if (status && status >= 400 && status < 500) return failureCount < 1;
        return failureCount < 2;
      },
      retryDelay: getRetryDelay,
      refetchOnWindowFocus: false,
      staleTime: 2000,
    },
  },
});

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>
);
