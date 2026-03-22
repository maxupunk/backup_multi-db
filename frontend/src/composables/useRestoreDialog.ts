import type { Backup, Connection, ConnectionDatabase, RestoreMode } from '@/types/api'
import { computed, reactive, ref, watch } from 'vue'
import { backupsApi, connectionsApi } from '@/services/api'
import { useNotifier } from './useNotifier'

export type RestoreStep = 1 | 2

export interface RestoreFormOptions {
  mode: RestoreMode
  targetConnectionId: number | null
  targetDatabase: string
  noOwner: boolean
  noPrivileges: boolean
  noTablespaces: boolean
  noComments: boolean
  noCreateDb: boolean
  skipSafetyBackup: boolean
}

const DEFAULT_FORM: RestoreFormOptions = {
  mode: 'full',
  targetConnectionId: null,
  targetDatabase: '',
  noOwner: false,
  noPrivileges: false,
  noTablespaces: false,
  noComments: false,
  noCreateDb: false,
  skipSafetyBackup: false,
}

/**
 * Composable que encapsula toda a lógica do diálogo de restauração.
 * Segue SRP: responsável apenas pelo estado e ações do restore dialog.
 */
export function useRestoreDialog(onSuccess?: () => void) {
  const notify = useNotifier()

  // ── Estado ────────────────────────────────────────────────────────────────
  const isOpen = ref(false)
  const step = ref<RestoreStep>(1)
  const backup = ref<Backup | null>(null)
  const form = reactive<RestoreFormOptions>({ ...DEFAULT_FORM })
  const restoreLoading = ref(false)

  // Conexões disponíveis passadas de fora (para evitar re-fetch)
  const availableConnections = ref<Connection[]>([])

  // Databases da conexão de destino selecionada
  const targetDatabases = ref<ConnectionDatabase[]>([])
  const loadingDatabases = ref(false)

  // Campo de confirmação (o usuário digita o nome do banco)
  const confirmationInput = ref('')

  // ── Computados ────────────────────────────────────────────────────────────
  const selectedConnection = computed<Connection | null>(() =>
    availableConnections.value.find((c) => c.id === form.targetConnectionId) ?? null,
  )

  const targetDatabaseName = computed<string>(() =>
    form.targetDatabase || backup.value?.databaseName || '',
  )

  const targetDbType = computed(() =>
    selectedConnection.value?.type ?? backup.value?.connection?.type ?? null,
  )

  const isConfirmationValid = computed<boolean>(() =>
    confirmationInput.value.trim() === targetDatabaseName.value.trim(),
  )

  const isDifferentConnection = computed<boolean>(() =>
    form.targetConnectionId !== null && form.targetConnectionId !== backup.value?.connectionId,
  )

  // Databases como lista de strings para o v-select
  const targetDatabaseItems = computed(() =>
    targetDatabases.value
      .filter((db) => db.enabled)
      .map((db) => db.databaseName),
  )

  // ── Ações ─────────────────────────────────────────────────────────────────
  function open(item: Backup, connections: Connection[]) {
    backup.value = item
    availableConnections.value = connections

    Object.assign(form, { ...DEFAULT_FORM })
    form.targetConnectionId = item.connectionId

    confirmationInput.value = ''
    step.value = 1

    _loadDatabasesForConnection(item.connectionId)
    isOpen.value = true
  }

  function close() {
    isOpen.value = false
  }

  function goToConfirmation() {
    confirmationInput.value = ''
    step.value = 2
  }

  function goBack() {
    step.value = 1
  }

  /**
   * Adiciona um banco de dados recém-criado à lista local e o seleciona.
   * Chamado após o CreateDatabaseDialog emitir o evento `created`.
   */
  function addDatabase(databaseName: string) {
    const alreadyListed = targetDatabases.value.some((db) => db.databaseName === databaseName)
    if (!alreadyListed) {
      targetDatabases.value = [...targetDatabases.value, { id: -1, databaseName, enabled: true }]
    }
    form.targetDatabase = databaseName
  }

  async function execute() {
    if (!backup.value || !isConfirmationValid.value) return

    restoreLoading.value = true
    try {
      const payload = _buildPayload()
      await backupsApi.restore(backup.value.id, payload)
      notify('Restauração iniciada. Acompanhe o progresso no painel.', 'info')
      close()
      onSuccess?.()
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : 'Erro ao restaurar backup'
      notify(msg, 'error')
    } finally {
      restoreLoading.value = false
    }
  }

  // ── Privados ──────────────────────────────────────────────────────────────
  async function _loadDatabasesForConnection(connectionId: number) {
    loadingDatabases.value = true
    targetDatabases.value = []

    try {
      const response = await connectionsApi.get(connectionId)
      targetDatabases.value = response.data?.databases ?? []
    } catch {
      // Sem databases — o campo ficará livre para digitação
    } finally {
      loadingDatabases.value = false
    }
  }

  function _buildPayload(): Record<string, unknown> {
    const payload: Record<string, unknown> = {
      mode: form.mode,
    }

    if (isDifferentConnection.value) {
      payload.targetConnectionId = form.targetConnectionId
    }

    if (form.targetDatabase) {
      payload.targetDatabase = form.targetDatabase
    }

    if (form.skipSafetyBackup) {
      payload.skipSafetyBackup = true
    }

    if (targetDbType.value === 'postgresql') {
      if (form.noOwner) payload.noOwner = true
      if (form.noPrivileges) payload.noPrivileges = true
      if (form.noTablespaces) payload.noTablespaces = true
      if (form.noComments) payload.noComments = true
    }

    if (
      (targetDbType.value === 'mysql' || targetDbType.value === 'mariadb') &&
      form.noCreateDb
    ) {
      payload.noCreateDb = true
    }

    return payload
  }

  // Recarrega databases ao trocar conexão
  watch(
    () => form.targetConnectionId,
    (newId) => {
      if (newId !== null) {
        form.targetDatabase = ''
        _loadDatabasesForConnection(newId)
      }
    },
  )

  return {
    // Estado
    isOpen,
    step,
    backup,
    form,
    restoreLoading,
    availableConnections,
    targetDatabases,
    targetDatabaseItems,
    loadingDatabases,
    confirmationInput,
    // Computados
    selectedConnection,
    targetDatabaseName,
    targetDbType,
    isConfirmationValid,
    isDifferentConnection,
    // Ações
    open,
    close,
    goToConfirmation,
    goBack,
    addDatabase,
    execute,
  }
}
