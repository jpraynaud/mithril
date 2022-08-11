# Default values
if [ -z "${ROOT}" ]; then 
  ROOT="artifacts"
fi
if [ -z "${NODES}" ]; then 
  NODES="*"
fi

# Change directory
cd ${ROOT}

# Query devnet
echo "====================================================================="
echo " Query Mithril/Cardano devnet"
echo "====================================================================="
echo
if [ "${NODES}" = "mithril" ] || [ "${NODES}" = "*" ]; then 
    echo "====================================================================="
    echo "=== Mithril Network"
    echo "====================================================================="
    echo
    ./query-mithril.sh
    echo
fi
if [ "${NODES}" = "cardano" ] || [ "${NODES}" = "*" ]; then 
    echo "====================================================================="
    echo "=== Cardano Network"
    echo "====================================================================="
    echo
    ./query-cardano.sh
    echo
fi