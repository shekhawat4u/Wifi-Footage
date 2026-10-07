#include <Arduino.h>
#include <WiFi.h>
#include <WiFiUdp.h>
#include <freertos/FreeRTOS.h>
#include <freertos/queue.h>
#include "esp_wifi.h"

const char* ssid = "Sumit_Bada";
const char* password = "deepbasics@123";
const char* receiver_id = "receiver-1";
const char* udp_server_ip = "192.168.0.141";
const int udp_server_port = 5005;

struct CSIPacket {
    int len;
    char data[800];
};

static QueueHandle_t csi_queue = NULL;
static WiFiUDP udp;

void csi_rx_cb(void *ctx, wifi_csi_info_t *info) {
    if (!info || !info->buf || info->len < 2 || !csi_queue) return;
    const wifi_pkt_rx_ctrl_t *rx = &info->rx_ctrl;

    CSIPacket pkt;
    int offset = snprintf(pkt.data, sizeof(pkt.data),
                          "CSI_DATA,%s,%lu,%02x:%02x:%02x:%02x:%02x:%02x,%d,%d,%d,%d",
                          receiver_id, millis(),
                          info->mac[0], info->mac[1], info->mac[2], info->mac[3], info->mac[4], info->mac[5],
                          rx->rssi, rx->noise_floor, rx->channel, info->len);

    for (int i = 0; i < info->len && offset < (int)sizeof(pkt.data) - 8; ++i) {
        offset += snprintf(pkt.data + offset, sizeof(pkt.data) - offset, ",%d", info->buf[i]);
    }
    if (offset < (int)sizeof(pkt.data) - 2) {
        pkt.data[offset++] = '\n';
        pkt.data[offset] = '\0';
    }
    pkt.len = offset;

    // Non-blocking push from ISR/callback to queue
    xQueueSendFromISR(csi_queue, &pkt, NULL);
}

void setup() {
    Serial.begin(921600);
    delay(200);
    Serial.println("\n[RF CSI Monitor] Initializing Wireless Station...");

    csi_queue = xQueueCreate(64, sizeof(CSIPacket));

    WiFi.mode(WIFI_STA);
    WiFi.setSleep(false); // Disable Wi-Fi power-save sleep to prevent stalling
    WiFi.begin(ssid, password);

    Serial.print("[Wi-Fi] Connecting to ");
    Serial.println(ssid);

    while (WiFi.status() != WL_CONNECTED) {
        delay(300);
        Serial.print(".");
    }

    Serial.println("\n[Wi-Fi] Connected! Local IP: " + WiFi.localIP().toString());
    udp.begin(udp_server_port);

    wifi_csi_config_t csi_config = {
        .lltf_en = true,
        .htltf_en = true,
        .stbc_htltf2_en = true,
        .ltf_merge_en = true,
        .channel_filter_en = true,
        .manu_scale = false,
        .shift = false,
    };

    ESP_ERROR_CHECK(esp_wifi_set_csi_config(&csi_config));
    ESP_ERROR_CHECK(esp_wifi_set_csi_rx_cb(csi_rx_cb, NULL));
    ESP_ERROR_CHECK(esp_wifi_set_csi(true));

    Serial.printf("[CSI] Initialized -> Wireless UDP Stream to %s:%d\n", udp_server_ip, udp_server_port);
}

void loop() {
    // Reconnect Wi-Fi if dropped
    if (WiFi.status() != WL_CONNECTED) {
        WiFi.reconnect();
        delay(500);
        return;
    }

    // Process queued CSI frames and stream wirelessly over UDP
    CSIPacket pkt;
    int sent_count = 0;
    while (xQueueReceive(csi_queue, &pkt, 0) == pdTRUE && sent_count < 20) {
        Serial.print(pkt.data);
        udp.beginPacket(udp_server_ip, udp_server_port);
        udp.write((const uint8_t*)pkt.data, pkt.len);
        udp.endPacket();
        sent_count++;
    }

    // High-rate active heartbeat to keep Wi-Fi router transmitting packets constantly
    static unsigned long last_ping = 0;
    if (millis() - last_ping > 40) { // 25 times per second
        last_ping = millis();
        udp.beginPacket(udp_server_ip, udp_server_port);
        udp.write((const uint8_t*)"PING\n", 5);
        udp.endPacket();
    }

    delay(2);
}

