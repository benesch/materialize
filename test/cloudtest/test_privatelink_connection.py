# Copyright Materialize, Inc. and contributors. All rights reserved.
#
# Use of this software is governed by the Business Source License
# included in the LICENSE file at the root of this repository.
#
# As of the Change Date specified in that file, in accordance with
# the Business Source License, use of this software will be governed
# by the Apache License, Version 2.0.

from textwrap import dedent

import pg8000
import pytest

from materialize.cloudtest.application import MaterializeApplication
from materialize.cloudtest.exists import exists


def test_create_privatelink_connection(mz: MaterializeApplication) -> None:
    # Create a PrivateLink SQL connection object,
    # which should create a K8S VpcEndpoint object.
    # We don't run the environment-controller,
    # so no AWS VPC Endpoint will be created.
    # so we don't need the named service to actually exist.
    create_connection_statement = dedent(
        """\
        CREATE CONNECTION privatelinkconn
        TO AWS PRIVATELINK (
            SERVICE NAME 'com.amazonaws.vpce.us-east-1.vpce-svc-0e123abc123198abc',
            AVAILABILITY ZONES ('use1-az1', 'use1-az4')
        )
        """
    )

    # this should fail until max_aws_privatelink_connections is increased
    with pytest.raises(pg8000.dbapi.ProgrammingError):
        mz.environmentd.sql(create_connection_statement)

    mz.environmentd.sql(
        "ALTER SYSTEM SET max_aws_privatelink_connections = 5",
        port_name="internal",
        user="mz_system",
    )
    mz.environmentd.sql(create_connection_statement)

    aws_connection_id = mz.environmentd.sql_query(
        "SELECT id FROM mz_connections WHERE name = 'privatelinkconn'"
    )[0][0]

    exists(resource=f"vpcendpoint/connection-{aws_connection_id}")

    mz.environmentd.sql(
        dedent(
            """\
            CREATE CONNECTION kafkaconn TO KAFKA (
            BROKERS (
                'customer-hostname-1:9092' USING AWS PRIVATELINK privatelinkconn,
                'customer-hostname-2:9092' USING AWS PRIVATELINK privatelinkconn (PORT 9093),
                'customer-hostname-4:9094',
            )
            );
            """
        )
    )
    mz.environmentd.sql_query("SELECT id FROM mz_connections WHERE name = 'kafkaconn'")[
        0
    ][0]

    mz.environmentd.sql("DELETE CONNECTION kafkaconn")
    mz.environmentd.sql("DELETE CONNECTION privatelinkconn")
    # TODO wait for the K8S resource to be deleted?
