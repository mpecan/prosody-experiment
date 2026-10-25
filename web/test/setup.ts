// Each test starts from a clean page: nothing rendered, nothing remembered.
import { cleanup } from '@testing-library/preact';
import { afterEach, beforeEach } from 'vitest';

beforeEach(() => localStorage.clear());
afterEach(() => cleanup());
