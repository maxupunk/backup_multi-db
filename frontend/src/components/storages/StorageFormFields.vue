<template>
  <!-- S3-based: aws_s3, minio, cloudflare_r2 -->
  <template v-if="isS3Based">
    <v-row>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.region"
          label="Região *"
          prepend-inner-icon="mdi-earth"
          :rules="[rules.required]"
          @update:model-value="emit('update:config', { ...config, region: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.bucket"
          label="Bucket *"
          prepend-inner-icon="mdi-bucket"
          :rules="[rules.required]"
          @update:model-value="emit('update:config', { ...config, bucket: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.accessKeyId"
          label="Access Key ID *"
          prepend-inner-icon="mdi-key"
          :rules="[rules.required]"
          @update:model-value="emit('update:config', { ...config, accessKeyId: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.secretAccessKey"
          label="Secret Access Key *"
          prepend-inner-icon="mdi-lock"
          :rules="[rules.required]"
          type="password"
          @update:model-value="emit('update:config', { ...config, secretAccessKey: $event })"
        />
      </v-col>
      <v-col v-if="props.provider !== 'aws_s3'" cols="12" sm="6">
        <v-text-field
          :model-value="config.endpoint"
          :label="props.provider === 'cloudflare_r2' ? 'Endpoint R2 *' : 'Endpoint *'"
          :placeholder="props.provider === 'cloudflare_r2' ? 'https://<account-id>.r2.cloudflarestorage.com' : 'https://minio.example.com:9000'"
          prepend-inner-icon="mdi-link-variant"
          :rules="[rules.required]"
          @update:model-value="emit('update:config', { ...config, endpoint: $event })"
        />
      </v-col>
      <v-col v-else cols="12" sm="6">
        <v-text-field
          :model-value="config.endpoint"
          label="Endpoint (opcional)"
          prepend-inner-icon="mdi-link-variant"
          @update:model-value="emit('update:config', { ...config, endpoint: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.prefix"
          label="Prefix (opcional)"
          prepend-inner-icon="mdi-folder-outline"
          @update:model-value="emit('update:config', { ...config, prefix: $event })"
        />
      </v-col>
      <v-col v-if="props.provider !== 'cloudflare_r2'" cols="12">
        <v-switch
          :model-value="config.forcePathStyle"
          color="primary"
          hide-details
          label="Force path style"
          @update:model-value="emit('update:config', { ...config, forcePathStyle: $event })"
        />
      </v-col>
    </v-row>
  </template>

  <!-- Google Cloud Storage -->
  <template v-else-if="props.provider === 'google_gcs'">
    <v-row>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.bucket"
          label="Bucket *"
          prepend-inner-icon="mdi-bucket"
          :rules="[rules.required]"
          @update:model-value="emit('update:config', { ...config, bucket: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.projectId"
          label="Project ID"
          prepend-inner-icon="mdi-identifier"
          @update:model-value="emit('update:config', { ...config, projectId: $event })"
        />
      </v-col>
      <v-col cols="12">
        <v-textarea
          :model-value="config.credentialsJson"
          auto-grow
          label="Credentials JSON"
          prepend-inner-icon="mdi-file-code"
          rows="3"
          @update:model-value="emit('update:config', { ...config, credentialsJson: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.prefix"
          label="Prefix (opcional)"
          prepend-inner-icon="mdi-folder-outline"
          @update:model-value="emit('update:config', { ...config, prefix: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-switch
          :model-value="config.usingUniformAcl"
          color="primary"
          hide-details
          label="Using uniform ACL"
          @update:model-value="emit('update:config', { ...config, usingUniformAcl: $event })"
        />
      </v-col>
    </v-row>
  </template>

  <!-- Azure Blob -->
  <template v-else-if="props.provider === 'azure_blob'">
    <v-row>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.container"
          label="Container *"
          prepend-inner-icon="mdi-package-variant"
          :rules="[rules.required]"
          @update:model-value="emit('update:config', { ...config, container: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.prefix"
          label="Prefix (opcional)"
          prepend-inner-icon="mdi-folder-outline"
          @update:model-value="emit('update:config', { ...config, prefix: $event })"
        />
      </v-col>
      <v-col cols="12">
        <v-textarea
          :model-value="config.connectionString"
          auto-grow
          label="Connection String *"
          prepend-inner-icon="mdi-lock"
          :rules="[rules.required]"
          rows="3"
          @update:model-value="emit('update:config', { ...config, connectionString: $event })"
        />
      </v-col>
    </v-row>
  </template>

  <!-- SFTP -->
  <template v-else-if="props.provider === 'sftp'">
    <v-row>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.host"
          label="Host *"
          prepend-inner-icon="mdi-server"
          :rules="[rules.required]"
          @update:model-value="emit('update:config', { ...config, host: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.port"
          label="Porta"
          prepend-inner-icon="mdi-ethernet"
          :rules="[rules.portOptional]"
          type="number"
          @update:model-value="emit('update:config', { ...config, port: Number($event) })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.username"
          label="Usuário *"
          prepend-inner-icon="mdi-account"
          :rules="[rules.required]"
          @update:model-value="emit('update:config', { ...config, username: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.password"
          label="Senha"
          prepend-inner-icon="mdi-lock"
          type="password"
          @update:model-value="emit('update:config', { ...config, password: $event })"
        />
      </v-col>
      <v-col cols="12">
        <v-textarea
          :model-value="config.privateKey"
          auto-grow
          label="Private Key"
          prepend-inner-icon="mdi-key"
          rows="3"
          @update:model-value="emit('update:config', { ...config, privateKey: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.passphrase"
          label="Passphrase"
          prepend-inner-icon="mdi-lock-outline"
          type="password"
          @update:model-value="emit('update:config', { ...config, passphrase: $event })"
        />
      </v-col>
      <v-col cols="12" sm="6">
        <v-text-field
          :model-value="config.basePath"
          label="Base Path"
          prepend-inner-icon="mdi-folder"
          @update:model-value="emit('update:config', { ...config, basePath: $event })"
        />
      </v-col>
    </v-row>
  </template>

  <!-- Local -->
  <template v-else-if="props.provider === 'local'">
    <v-text-field
      :model-value="config.basePath"
      label="Base Path"
      :placeholder="DEFAULT_LOCAL_PATH"
      prepend-inner-icon="mdi-folder"
      @update:model-value="emit('update:config', { ...config, basePath: $event })"
    />
    <v-alert
      v-if="config.basePath && String(config.basePath).trim() !== DEFAULT_LOCAL_PATH"
      class="mt-2"
      density="compact"
      icon="mdi-alert-circle"
      type="warning"
      variant="tonal"
    >
      <strong>Atenção:</strong> Somente a pasta <code>{{ DEFAULT_LOCAL_PATH }}</code> é salva permanentemente.
      Backups em outros caminhos podem ser <strong>perdidos</strong> ao reiniciar o sistema.
    </v-alert>
  </template>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { StorageProvider } from '@/types/api'

const DEFAULT_LOCAL_PATH = '/storage/backups'

const props = defineProps<{
  provider: StorageProvider
  config: Record<string, unknown>
}>()

const emit = defineEmits<{
  'update:config': [value: Record<string, unknown>]
}>()

const isS3Based = computed(() =>
  ['aws_s3', 'minio', 'cloudflare_r2'].includes(props.provider),
)

const rules = {
  required: (v: unknown) => {
    const value = typeof v === 'string' ? v.trim() : v
    return !!value || 'Campo obrigatório'
  },
  portOptional: (v: unknown) => {
    if (v === null || v === undefined || v === '') return true
    const value = typeof v === 'number' ? v : Number(v)
    if (Number.isNaN(value)) return 'Porta inválida'
    return (value > 0 && value <= 65_535) || 'Porta inválida'
  },
}
</script>
