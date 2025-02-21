/*******************************************************************************
*   (c) 2018, 2019 Zondax GmbH
*   (c) 2016 Ledger
*
*  Licensed under the Apache License, Version 2.0 (the "License");
*  you may not use this file except in compliance with the License.
*  You may obtain a copy of the License at
*
*      http://www.apache.org/licenses/LICENSE-2.0
*
*  Unless required by applicable law or agreed to in writing, software
*  distributed under the License is distributed on an "AS IS" BASIS,
*  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*  See the License for the specific language governing permissions and
*  limitations under the License.
********************************************************************************/

#include "app_main.h"

#include <string.h>
#include <os_io_seproxyhal.h>
#include <os.h>

#include "view.h"
#include "actions.h"
#include "tx.h"
#include "addr.h"
#include "crypto.h"
#include "coin.h"
#include "zxmacros.h"

#define REPLY_APDU 0x03

__Z_INLINE void handleGetAddrSecp256K1(volatile uint32_t *flags, volatile uint32_t *tx, uint32_t rx) {
    extract_default_path(rx, OFFSET_DATA);

    uint8_t requireConfirmation = G_io_apdu_buffer[OFFSET_P1];
    uint8_t network = G_io_apdu_buffer[OFFSET_P2];

    // Set the address version
    if (!set_network_version(network))
        return THROW(APDU_CODE_DATA_INVALID);

    if (requireConfirmation) {
        app_fill_address(addr_secp256k1);

        view_review_init(addr_getItem, addr_getNumItems, app_reply_address);
        view_review_show(REPLY_APDU);

        *flags |= IO_ASYNCH_REPLY;
        return;
    }

    *tx = app_fill_address(addr_secp256k1);
    THROW(APDU_CODE_OK);
}

__Z_INLINE void handleGetAuthPubKey(volatile uint32_t *flags, volatile uint32_t *tx, uint32_t rx) {
    extract_identity_path(rx, OFFSET_DATA);

    *tx = app_fill_auth_pubkey(addr_secp256k1);
    THROW(APDU_CODE_OK);
}

__Z_INLINE void SignSecp256K1(volatile uint32_t *flags, volatile uint32_t *tx, uint32_t rx) {
    // process the rest of the chunk as usual
    if (!process_chunk(rx)) {
        THROW(APDU_CODE_OK);
    }

    const char *error_msg = tx_parse();

    if (error_msg != NULL) {
        int error_msg_length = strlen(error_msg);
        MEMCPY(G_io_apdu_buffer, error_msg, error_msg_length);
        *tx += (error_msg_length);
        THROW(APDU_CODE_DATA_INVALID);
    }

    zemu_log_stack("tx_parse done\n");

    CHECK_APP_CANARY()
    view_review_init(tx_getItem, tx_getNumItems, app_sign);
    view_review_show(REPLY_APDU);
    *flags |= IO_ASYNCH_REPLY;
}


__Z_INLINE void handleSignSecp256K1(volatile uint32_t *flags, volatile uint32_t *tx, uint32_t rx) {
    // check first for the expected path at initialization
    if (G_io_apdu_buffer[OFFSET_PAYLOAD_TYPE] == 0) {
        extract_default_path(rx, OFFSET_DATA);
    }

    SignSecp256K1(flags, tx, rx);
}

__Z_INLINE void handleSignJwtSecp256K1(volatile uint32_t *flags, volatile uint32_t *tx, uint32_t rx) {
    // check first for the expected path at initialization
    if (G_io_apdu_buffer[OFFSET_PAYLOAD_TYPE] == 0) {
        extract_identity_path(rx, OFFSET_DATA);
    }

    SignSecp256K1(flags, tx, rx);
}

void handleApdu(volatile uint32_t *flags, volatile uint32_t *tx, uint32_t rx) {
    uint16_t sw = 0;

    BEGIN_TRY
    {
        TRY
        {
            if (G_io_apdu_buffer[OFFSET_CLA] != CLA) {
                THROW(APDU_CODE_CLA_NOT_SUPPORTED);
            }

            if (rx < APDU_MIN_LENGTH) {
                THROW(APDU_CODE_WRONG_LENGTH);
            }

            switch (G_io_apdu_buffer[OFFSET_INS]) {
                case INS_GET_VERSION: {
                    handle_getversion(flags, tx, rx);
                    break;
                }

                case INS_GET_ADDR_SECP256K1: {
                    if (os_global_pin_is_validated() != BOLOS_UX_OK) {
                        THROW(APDU_CODE_COMMAND_NOT_ALLOWED);
                    }
                    handleGetAddrSecp256K1(flags, tx, rx);
                    break;
                }

                case INS_GET_AUTH_PUBKEY: {
                    if (os_global_pin_is_validated() != BOLOS_UX_OK) {
                        THROW(APDU_CODE_COMMAND_NOT_ALLOWED);
                    }
                    handleGetAuthPubKey(flags, tx, rx);
                    break;
                }

                case INS_SIGN_SECP256K1: {
                    if (os_global_pin_is_validated() != BOLOS_UX_OK) {
                        THROW(APDU_CODE_COMMAND_NOT_ALLOWED);
                    }
                    handleSignSecp256K1(flags, tx, rx);
                    break;
                }

                case SIGN_JWT_SECP256K1: {
                    if (os_global_pin_is_validated() != BOLOS_UX_OK) {
                        THROW(APDU_CODE_COMMAND_NOT_ALLOWED);
                    }
                    handleSignJwtSecp256K1(flags, tx, rx);
                    break;
                }

                default:
                    THROW(APDU_CODE_INS_NOT_SUPPORTED);
            }
        }
        CATCH(EXCEPTION_IO_RESET)
        {
            THROW(EXCEPTION_IO_RESET);
        }
        CATCH_OTHER(e)
        {
            switch (e & 0xF000) {
                case 0x6000:
                case APDU_CODE_OK:
                    sw = e;
                    break;
                default:
                    sw = 0x6800 | (e & 0x7FF);
                    break;
            }
            G_io_apdu_buffer[*tx] = sw >> 8;
            G_io_apdu_buffer[*tx + 1] = sw;
            *tx += 2;
        }
        FINALLY
        {
        }
    }
    END_TRY;
}
