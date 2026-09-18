{{/*
Common naming and address helpers for the DeTTa chart.
*/}}
{{- define "detta.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "detta.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := include "detta.name" . -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "detta.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "detta.labels" -}}
helm.sh/chart: {{ include "detta.chart" . }}
app.kubernetes.io/name: {{ include "detta.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{- define "detta.selectorLabels" -}}
app.kubernetes.io/name: {{ include "detta.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "detta.image" -}}
{{- $repository := required "image.repository is required" .Values.image.repository -}}
{{- $tag := default .Chart.AppVersion .Values.image.tag -}}
{{- if .Values.image.registry -}}
{{- printf "%s/%s:%s" .Values.image.registry $repository $tag -}}
{{- else -}}
{{- printf "%s:%s" $repository $tag -}}
{{- end -}}
{{- end -}}

{{- define "detta.validatorId" -}}
{{- printf "validator-%d" (int .) -}}
{{- end -}}

{{- define "detta.validatorName" -}}
{{- printf "%s-validator-%d" (include "detta.fullname" .root) (int .index) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "detta.serviceEnvPrefix" -}}
{{- . | upper | replace "-" "_" -}}
{{- end -}}

{{- define "detta.rpcName" -}}
{{- printf "%s-rpc" (include "detta.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "detta.consensusSecretName" -}}
{{- if .Values.consensus.seed.existingSecret -}}
{{- .Values.consensus.seed.existingSecret -}}
{{- else -}}
{{- printf "%s-consensus" (include "detta.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "detta.validatorRoster" -}}
{{- $ids := list -}}
{{- range $i := until (int .Values.validators.count) -}}
{{- $ids = append $ids (printf "validator-%d" (add $i 1)) -}}
{{- end -}}
{{- join "," $ids -}}
{{- end -}}

{{- define "detta.validatorRpcPeers" -}}
{{- $root := .root -}}
{{- $self := int .self -}}
{{- $peers := list -}}
{{- range $i := until (int $root.Values.validators.count) -}}
{{- $idx := add $i 1 -}}
{{- if ne $idx $self -}}
{{- $peers = append $peers (printf "%s:%d" (include "detta.validatorName" (dict "root" $root "index" $idx)) (int $root.Values.rpc.port)) -}}
{{- end -}}
{{- end -}}
{{- join "," $peers -}}
{{- end -}}

{{- define "detta.validatorRpcPeersRuntime" -}}
{{- $root := .root -}}
{{- $self := int .self -}}
{{- $peers := list -}}
{{- range $i := until (int $root.Values.validators.count) -}}
{{- $idx := add $i 1 -}}
{{- if ne $idx $self -}}
{{- $service := include "detta.validatorName" (dict "root" $root "index" $idx) -}}
{{- $envPrefix := include "detta.serviceEnvPrefix" $service -}}
{{- $peers = append $peers (printf "$(detta_service_host %s_SERVICE_HOST %s):%d" $envPrefix $service (int $root.Values.rpc.port)) -}}
{{- end -}}
{{- end -}}
{{- join "," $peers -}}
{{- end -}}

{{- define "detta.validatorRpcPeerTargetsRuntime" -}}
{{- $root := .root -}}
{{- $self := int .self -}}
{{- $peers := list -}}
{{- range $i := until (int $root.Values.validators.count) -}}
{{- $idx := add $i 1 -}}
{{- if ne $idx $self -}}
{{- $service := include "detta.validatorName" (dict "root" $root "index" $idx) -}}
{{- $envPrefix := include "detta.serviceEnvPrefix" $service -}}
{{- $peers = append $peers (printf "$(detta_service_host %s_SERVICE_HOST %s):%d" $envPrefix $service (int $root.Values.rpc.port)) -}}
{{- end -}}
{{- end -}}
{{- join " " $peers -}}
{{- end -}}

{{- define "detta.allValidatorRpcPeers" -}}
{{- $peers := list -}}
{{- range $i := until (int .Values.validators.count) -}}
{{- $idx := add $i 1 -}}
{{- $peers = append $peers (printf "%s:%d" (include "detta.validatorName" (dict "root" $ "index" $idx)) (int $.Values.rpc.port)) -}}
{{- end -}}
{{- join "," $peers -}}
{{- end -}}

{{- define "detta.allValidatorRpcPeersRuntime" -}}
{{- $peers := list -}}
{{- range $i := until (int .Values.validators.count) -}}
{{- $idx := add $i 1 -}}
{{- $service := include "detta.validatorName" (dict "root" $ "index" $idx) -}}
{{- $envPrefix := include "detta.serviceEnvPrefix" $service -}}
{{- $peers = append $peers (printf "$(detta_service_host %s_SERVICE_HOST %s):%d" $envPrefix $service (int $.Values.rpc.port)) -}}
{{- end -}}
{{- join "," $peers -}}
{{- end -}}

{{- define "detta.validatorConsensusPeers" -}}
{{- $root := .root -}}
{{- $self := int .self -}}
{{- $peers := list -}}
{{- range $i := until (int $root.Values.validators.count) -}}
{{- $idx := add $i 1 -}}
{{- if ne $idx $self -}}
{{- $peers = append $peers (printf "validator-%d=%s:%d" $idx (include "detta.validatorName" (dict "root" $root "index" $idx)) (int $root.Values.consensus.port)) -}}
{{- end -}}
{{- end -}}
{{- join "," $peers -}}
{{- end -}}

{{- define "detta.validatorConsensusPeersRuntime" -}}
{{- $root := .root -}}
{{- $self := int .self -}}
{{- $peers := list -}}
{{- range $i := until (int $root.Values.validators.count) -}}
{{- $idx := add $i 1 -}}
{{- if ne $idx $self -}}
{{- $service := include "detta.validatorName" (dict "root" $root "index" $idx) -}}
{{- $envPrefix := include "detta.serviceEnvPrefix" $service -}}
{{- $peers = append $peers (printf "validator-%d=$(detta_service_host %s_SERVICE_HOST %s):%d" $idx $envPrefix $service (int $root.Values.consensus.port)) -}}
{{- end -}}
{{- end -}}
{{- join "," $peers -}}
{{- end -}}

{{- define "detta.probe" -}}
exec:
  command:
    - /usr/local/bin/detta-client
    - state-root
    - --rpc
    - 127.0.0.1:{{ .Values.rpc.port }}
{{- end -}}

{{- define "detta.storageClass" -}}
{{- if .storageClass -}}
{{- if eq .storageClass "-" }}
storageClassName: ""
{{- else }}
storageClassName: {{ .storageClass | quote }}
{{- end -}}
{{- end -}}
{{- end -}}
