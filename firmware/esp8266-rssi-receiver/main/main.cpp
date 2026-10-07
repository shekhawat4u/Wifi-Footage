#include <ESP8266WiFi.h>
#include <WiFiUdp.h>

const char* ssid = "Sumit_Bada";
const char* password = "deepbasics@123";
const char* receiver_id = "receiver-2";
const char* udp_server_ip = "192.168.0.141";
const int udp_server_port = 5005;

WiFiUDP udp;
char packet_buf[256];

void setup() {
    Serial.begin(115200);
    delay(200);
    Serial.println("\n[RF Node 2 - ESP8266] Initializing...");

    WiFi.mode(WIFI_STA);
    WiFi.begin(ssid, password);

    Serial.print("[Wi-Fi] Connecting to ");
    Serial.println(ssid);

    while (WiFi.status() != WL_CONNECTED) {
        delay(300);
        Serial.print(".");
    }

    Serial.println("\n[Wi-Fi] Connected! IP: " + WiFi.localIP().toString());
    udp.begin(udp_server_port);
}

void loop() {
    if (WiFi.status() != WL_CONNECTED) {
        WiFi.reconnect();
        delay(500);
        return;
    }

    static unsigned long last_frame = 0;
    if (millis() - last_frame >= 50) { // 20Hz sampling
        last_frame = millis();

        int rssi = WiFi.RSSI();
        // Generate standard project CSV frame
        int offset = snprintf(packet_buf, sizeof(packet_buf),
                              "CSI_DATA,%s,%lu,dc:ea:e7:39:ae:3d,%d,-96,6,16",
                              receiver_id, millis(), rssi);

        // Synthesize 16 spectral bins from RSSI variance & fading
        static float last_rssi = -60.0;
        float diff = abs(rssi - last_rssi);
        last_rssi = 0.8 * last_rssi + 0.2 * rssi;
        int var_amp = (int)(diff * 8.0);

        for (int i = 0; i < 16; ++i) {
            int val = (i % 2 == 0) ? (30 + var_amp) : (var_amp * 2);
            offset += snprintf(packet_buf + offset, sizeof(packet_buf) - offset, ",%d", val);
        }
        if (offset < (int)sizeof(packet_buf) - 2) {
            packet_buf[offset++] = '\n';
            packet_buf[offset] = '\0';
        }

        Serial.print(packet_buf);

        udp.beginPacket(udp_server_ip, udp_server_port);
        udp.write((const uint8_t*)packet_buf, offset);
        udp.endPacket();
    }

    delay(5);
}
